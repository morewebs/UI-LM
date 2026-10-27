//! Run-length codec for grids.
//!
//! A frame is a few thousand cells but usually only a few hundred transitions,
//! so it travels as horizontal runs of identical cells. The codec is lossless in
//! both directions: every cell of the grid, blanks included, is covered by some
//! run, and decoding a complete set of runs reproduces the grid exactly.

use serde::{Deserialize, Serialize};

use crate::grid::{Grid, Style, StyledCell};

/// A horizontal run of identical cells inside one row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRun {
    /// Row the run starts in.
    pub row: u16,
    /// Column the run starts in.
    pub col: u16,
    /// How many cells the run covers.
    pub len: u16,
    /// The character every cell in the run holds.
    pub ch: char,
    /// The style every cell in the run holds.
    pub style: Style,
    /// True when these cells are the continuation columns of wide glyphs.
    pub wide_tail: bool,
}

/// Encodes `grid` as runs, row by row and left to right within a row.
///
/// Every cell is covered, trailing blanks included: a styled blank is not the
/// same as an absent cell, so it cannot be dropped as empty space.
///
/// The wire format declares rows, columns and lengths as `u16`, so a grid taller
/// or wider than that is outside what this codec can represent at all.
pub fn encode_grid(grid: &Grid) -> Vec<CellRun> {
    let mut runs = Vec::new();
    for (row, cells) in grid.iter().enumerate() {
        let mut start = 0;
        while start < cells.len() {
            let mut end = start + 1;
            while end < cells.len() && cells[end] == cells[start] {
                end += 1;
            }
            let cell = &cells[start];
            runs.push(CellRun {
                row: row as u16,
                col: start as u16,
                len: (end - start) as u16,
                ch: cell.ch,
                style: cell.style.clone(),
                wide_tail: cell.wide_tail,
            });
            start = end;
        }
    }
    runs
}

/// Stamps `runs` onto a fresh `rows x cols` grid of blanks.
///
/// Rows and columns are always exactly as declared. A run overrunning the right
/// edge is truncated — that is where a wide glyph at the edge lands — and a run
/// outside the grid is ignored, so a malformed frame degrades to blanks instead
/// of panicking.
pub fn decode_grid(runs: &[CellRun], rows: u16, cols: u16) -> Grid {
    let mut grid = vec![vec![StyledCell::blank(); cols as usize]; rows as usize];
    for run in runs {
        let Some(line) = grid.get_mut(run.row as usize) else {
            continue;
        };
        let start = (run.col as usize).min(line.len());
        let end = (start + run.len as usize).min(line.len());
        for cell in &mut line[start..end] {
            cell.ch = run.ch;
            cell.style = run.style.clone();
            cell.wide_tail = run.wide_tail;
        }
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Style, StyledCell};

    #[test]
    fn roundtrip_preserves_every_cell_including_trailing_spaces() {
        // A 2x4 grid whose last row is entirely spaces but styled (reverse video),
        // which is what a selected-but-empty list row looks like.
        let mut g = vec![vec![StyledCell::blank(); 4]; 2];
        g[0][0] = StyledCell::new('a');
        g[0][1] = StyledCell::new('│'); // box drawing
        g[1][3] = StyledCell::new(' '); // trailing space that matters
        g[1][3].style = Style {
            reverse: true,
            ..Style::default()
        };

        let runs = encode_grid(&g);
        let back = decode_grid(&runs, 2, 4);
        assert_eq!(back, g);
        assert_eq!(back[1].len(), 4, "trailing cells must survive encoding");
        assert!(back[1][3].style.reverse, "style must survive on a space");
    }

    #[test]
    fn identical_neighbours_collapse_into_one_run() {
        let g = vec![vec![StyledCell::new('x'); 8]];
        let runs = encode_grid(&g);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].len, 8);
    }

    #[test]
    fn decode_pads_short_rows_with_blanks() {
        let back = decode_grid(&[], 2, 3);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].len(), 3);
        assert_eq!(back[0][0].ch, ' ');
    }

    #[test]
    fn decode_clips_runs_to_the_declared_grid() {
        // A wide glyph starting in the last column is wider than the row, and a
        // run naming a row that is not there cannot widen the grid either: a
        // malformed frame must degrade to blanks rather than panic.
        let runs = vec![
            CellRun {
                row: 0,
                col: 2,
                len: 4,
                ch: '界',
                style: Style::default(),
                wide_tail: false,
            },
            CellRun {
                row: 9,
                col: 0,
                len: 1,
                ch: 'x',
                style: Style::default(),
                wide_tail: false,
            },
        ];

        let back = decode_grid(&runs, 1, 3);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].len(), 3);
        assert_eq!(back[0][2].ch, '界', "the in-bounds part of the run is kept");
        assert_eq!(back[0][0], StyledCell::blank());
    }

    #[test]
    fn roundtrip_preserves_wide_tail() {
        // The continuation column of a full-width glyph is a space that is *not*
        // an ordinary blank; the codec has to carry that distinction.
        let mut g = vec![vec![StyledCell::blank(); 2]];
        g[0][0] = StyledCell::new('界');
        g[0][1].wide_tail = true;

        let runs = encode_grid(&g);
        let back = decode_grid(&runs, 1, 2);
        assert!(back[0][1].wide_tail, "wide-tail flag must survive encoding");
        assert_ne!(back[0][1], StyledCell::blank());
    }
}
