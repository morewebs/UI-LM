//! K5: does the emulator's screen hold what the application's own draw buffer
//! says it holds?
//!
//! Everything the rig produces is an assertion about a screen: this row held this
//! text in this style. The assertion is made by a VT100 emulator fed the bytes the
//! application wrote to its pty, and the application made the same assertion by
//! filling a buffer and telling the terminal to draw it. Where the two disagree,
//! every frame, number and training pair built from the emulator describes a screen
//! nobody ever saw — which is why this comparison is a kill criterion rather than a
//! unit test.
//!
//! The comparison is cell by cell, character and style. Only the character and the
//! style are compared, never the wide-glyph tail flag: our side derives that flag
//! from the emulator's column accounting, and the dumped side cannot report it at
//! all, so comparing it would fail every wide glyph for a fact one side does not
//! carry. What a wide glyph looks like is already covered by comparing the glyph's
//! own cell and the blank its continuation column holds.

use ui_lm_frames::{decode_grid, Frame, Span};

/// What a comparison found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Comparison {
    /// Cells the two screens are compared over: every cell of the shape they share,
    /// including the ones a mask kept out of the comparison.
    pub total_cells: u64,
    /// Cells whose character or style differed.
    pub mismatched: u64,
    /// The first disagreement in row-major order, as `(row, col, ours, theirs)`.
    ///
    /// One example is what makes a mismatch actionable: a count says the two sides
    /// disagree, and a position says where to look. The characters travel with it
    /// because a mismatch on a styled blank has no character to read off a report.
    pub first_mismatch: Option<(u16, u16, char, char)>,
}

/// Compares two screens cell by cell, and reports what differed.
///
/// The two frames are decoded to one shared shape — the taller of the two by the
/// wider of the two — and every cell of that shape is compared. Decoding is what
/// makes a malformed frame harmless: a run past the right edge is truncated and a
/// run naming a row the frame does not have is dropped, so the comparison reads
/// blanks where one side has no cell rather than indexing off the end. A screen
/// captured at a different size therefore reports its extra or missing cells as
/// mismatches, which is the honest reading of two screens that are not the same
/// shape.
///
/// A cell inside one of the `ignore` spans is skipped entirely: a volatile span is
/// a region whose changes were already classified as not meaning anything, and a
/// comparison that counted them would report a spinner as a failure of the
/// emulator. Masks apply to the *comparison* only — the cells stay counted in
/// [`Comparison::total_cells`] and stay in both frames, because a mask is a
/// statement about meaning, not permission to drop content.
///
/// The character and the style are both compared. The wide-glyph tail flag is not:
/// a dump cannot report it, so comparing it would fail every wide glyph in every
/// dump for a fact that side does not carry.
pub fn compare_frames(ours: &Frame, theirs: &Frame, ignore: &[Span]) -> Comparison {
    let rows = ours.rows.max(theirs.rows);
    let cols = ours.cols.max(theirs.cols);
    // Both grids come back exactly `rows x cols`, with every in-bounds run stamped
    // on them, so the indexing below cannot leave either grid.
    let our_grid = decode_grid(&ours.runs, rows, cols);
    let their_grid = decode_grid(&theirs.runs, rows, cols);

    let mut comparison = Comparison {
        total_cells: u64::from(rows) * u64::from(cols),
        mismatched: 0,
        first_mismatch: None,
    };
    // Row-major, so the reported example is the first disagreement as someone
    // looking at the screen would find it.
    for row in 0..rows {
        for col in 0..cols {
            if masked(ignore, row, col) {
                continue;
            }
            let mine = &our_grid[row as usize][col as usize];
            let other = &their_grid[row as usize][col as usize];
            if mine.ch == other.ch && mine.style == other.style {
                continue;
            }
            comparison.mismatched += 1;
            if comparison.first_mismatch.is_none() {
                comparison.first_mismatch = Some((row, col, mine.ch, other.ch));
            }
        }
    }
    comparison
}

/// Whether `(row, col)` is inside one of `spans`.
///
/// A span is one row's half-open range `col .. col + len`: it covers cells on its
/// own row only, an empty span covers nothing, and a span reaching past the edge of
/// the frame covers the cells that are there and no others.
fn masked(spans: &[Span], row: u16, col: u16) -> bool {
    spans
        .iter()
        .any(|span| span.row == row && col >= span.col && col - span.col < span.len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_lm_frames::{frame_from_chars, MaskReason};

    /// A frame of `chars`, every cell in the default style. `cols` is the declared
    /// width, so a row may be shorter than it or overrun it.
    fn frame(seq: u32, cols: u16, chars: &[Vec<char>]) -> Frame {
        frame_from_chars(seq, chars.len() as u16, cols, chars)
    }

    fn span(row: u16, col: u16, len: u16) -> Span {
        Span {
            row,
            col,
            len,
            reason: MaskReason::NoSemanticChange,
        }
    }

    #[test]
    fn identical_frames_match() {
        let f = frame(1, 2, &[vec!['a', 'b']]);
        let c: Comparison = compare_frames(&f, &f, &[]);
        assert_eq!(c.mismatched, 0);
        assert_eq!(c.total_cells, 2);
        assert_eq!(c.first_mismatch, None);
    }

    #[test]
    fn a_single_wrong_cell_is_reported_with_its_position() {
        let a = frame(1, 2, &[vec!['a', 'b']]);
        let b = frame(1, 2, &[vec!['a', 'x']]);
        let c = compare_frames(&a, &b, &[]);
        assert_eq!(c.mismatched, 1);
        assert_eq!(
            c.first_mismatch,
            Some((0, 1, 'b', 'x')),
            "the report must name the cell, and which side held what"
        );
        assert_eq!(c.total_cells, 2, "a mismatched cell is still a cell");
    }

    #[test]
    fn masked_spans_are_excluded_from_the_count() {
        let a = frame(1, 2, &[vec!['a', 'b']]);
        let b = frame(1, 2, &[vec!['a', 'x']]);
        let ignore = vec![span(0, 1, 1)];
        let c = compare_frames(&a, &b, &ignore);
        assert_eq!(c.mismatched, 0);
        assert_eq!(
            c.first_mismatch, None,
            "a cell inside a masked span is not compared at all"
        );
        assert_eq!(
            c.total_cells, 2,
            "masking removes cells from the comparison, not from the screen"
        );
    }

    #[test]
    fn a_masked_span_masks_only_the_cells_it_covers() {
        // The neighbours of a masked span are ordinary cells and one of them here
        // is wrong: a mask that overshot to the rest of the row, or one that matched
        // the whole row on a column hit, would report this screen as clean.
        let a = frame(1, 4, &[vec!['a', 'b', 'c', 'd']]);
        let b = frame(1, 4, &[vec!['a', 'x', 'y', 'd']]);
        let c = compare_frames(&a, &b, &[span(0, 1, 1)]);
        assert_eq!(c.mismatched, 1, "columns 0, 2 and 3 are still compared");
        assert_eq!(c.first_mismatch, Some((0, 2, 'c', 'y')));
    }

    #[test]
    fn style_differences_count_as_mismatches() {
        // The screen is the text *and* how it was drawn. A comparison that looked
        // only at characters would call a colour-coded error message identical to
        // its uncoloured twin, and the rig would record a screen whose styling it
        // never checked.
        let mut a = frame(1, 1, &[vec!['a']]);
        let mut b = frame(1, 1, &[vec!['a']]);
        b.runs[0].style.bold = !a.runs[0].style.bold;
        a.runs[0].style.bold = false;
        let c = compare_frames(&a, &b, &[]);
        assert_eq!(c.mismatched, 1);
        assert_eq!(
            c.first_mismatch,
            Some((0, 0, 'a', 'a')),
            "the position is reported even when only the style differs"
        );
    }

    #[test]
    fn the_first_mismatch_is_the_first_in_row_major_order() {
        // "First" has to mean first as a reader scans the screen: row 0 before row
        // 1, and within a row left to right. A walk that visited columns first would
        // report the later cell here.
        let a = frame(1, 2, &[vec!['a', 'b'], vec!['c', 'd']]);
        let b = frame(1, 2, &[vec!['a', 'X'], vec!['Y', 'd']]);
        let c = compare_frames(&a, &b, &[]);
        assert_eq!(c.mismatched, 2);
        assert_eq!(c.first_mismatch, Some((0, 1, 'b', 'X')));
    }

    #[test]
    fn a_shape_difference_degrades_to_mismatches_rather_than_panicking() {
        // The two sides are supposed to be run at the same size, but a dump taken at
        // the wrong size must say so, not crash the validator or silently compare the
        // overlap and call it clean. Cells one side does not have are compared as
        // blanks, and the screen is counted at its full extent.
        let ours = frame(1, 2, &[vec!['a', 'b']]);
        let theirs = frame(1, 3, &[vec!['a', 'b', 'c'], vec!['d', 'e', 'f']]);
        let c = compare_frames(&ours, &theirs, &[]);
        assert_eq!(c.total_cells, 6, "the union of the two screens is compared");
        assert_eq!(
            c.mismatched, 4,
            "the missing third column of row 0, then every cell of row 1"
        );
        assert_eq!(c.first_mismatch, Some((0, 2, ' ', 'c')));
    }

    #[test]
    fn an_empty_frame_compares_as_blank_cells() {
        // A frame with no runs is a screen of blanks rather than an error: the first
        // frame of a session has no history, and a comparison must still be defined
        // on whatever it is handed.
        let ours = Frame::empty(1, 2);
        let theirs = frame(1, 2, &[vec!['a', 'b']]);
        let c = compare_frames(&ours, &theirs, &[]);
        assert_eq!(c.total_cells, 2);
        assert_eq!(c.mismatched, 2);
    }
}
