//! The styled cell grid: a terminal frame as data rather than as a screen buffer.

use serde::{Deserialize, Serialize};

/// A colour as the terminal understood it.
///
/// `Default` means "whatever the terminal profile decides", which is why it is
/// distinct from black: a frame must record that no colour was requested.
///
/// The wire form is a `kind` discriminator plus the payload under `value`, e.g.
/// `{"kind":"indexed","value":1}`. Adjacent tagging is what serde can do with
/// tuple payloads: an internally tagged enum (`tag` alone) is rejected for tuple
/// variants at compile time, and cannot serialise a newtype over a primitive.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Color {
    /// No colour was requested.
    #[default]
    Default,
    /// One of the 256 palette entries.
    Indexed(u8),
    /// A 24-bit colour.
    Rgb(u8, u8, u8),
}

/// The attributes of a single cell.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    /// Foreground colour.
    pub fg: Color,
    /// Background colour.
    pub bg: Color,
    /// Bold or increased intensity.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Foreground and background swapped.
    pub reverse: bool,
}

/// One cell of the grid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledCell {
    /// The character in this cell. A continuation column holds a space.
    pub ch: char,
    /// How that character was drawn.
    pub style: Style,
    /// True when this cell is the continuation column of a full-width glyph.
    ///
    /// Such a cell is a space, but it is not a blank: it belongs to the glyph to
    /// its left. Losing the flag turns a wide glyph into a glyph plus a gap.
    pub wide_tail: bool,
}

impl StyledCell {
    /// A space with the default style and no wide-glyph tail.
    pub fn blank() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
            wide_tail: false,
        }
    }

    /// A cell holding `ch` with the default style and no wide-glyph tail.
    pub fn new(ch: char) -> Self {
        Self {
            ch,
            style: Style::default(),
            wide_tail: false,
        }
    }
}

/// A screen's worth of cells: `grid[row][col]`, always exactly `rows x cols`.
pub type Grid = Vec<Vec<StyledCell>>;

/// Reads `screen` into a grid of styled cells: the emulator's `rows` by the
/// caller's `cols`.
///
/// The emulator has already done the column accounting, and this adapter takes it
/// as given: a full-width glyph is where `vt100` put it, and the column after it
/// is recorded as a wide-glyph continuation rather than read as a blank.
/// Re-deriving widths from the character instead would make ambiguous-width
/// characters resolve differently per machine and locale, so the same bytes would
/// yield different geometry everywhere they were captured.
///
/// A cell the screen does not have becomes a blank, so the grid is always exactly
/// the declared shape and no geometry mistakes here reach the recorded frame.
pub fn grid_from_screen(screen: &vt100::Screen, cols: u16) -> Grid {
    let rows = screen.size().0;
    (0..rows)
        .map(|row| {
            (0..cols)
                .map(|col| styled_cell(screen.cell(row, col)))
                .collect()
        })
        .collect()
}

/// One emulator cell as a grid cell; a missing cell (past the screen's edge) is a
/// blank.
fn styled_cell(cell: Option<&vt100::Cell>) -> StyledCell {
    let Some(cell) = cell else {
        return StyledCell::blank();
    };
    // The continuation column of a wide glyph is a space whatever the emulator
    // holds there (nothing, a space, or the glyph itself): the flag, not the
    // character, is what says this cell belongs to the glyph on its left.
    let wide_tail = cell.is_wide_continuation();
    StyledCell {
        ch: if wide_tail { ' ' } else { first_char(cell) },
        style: style_of(cell),
        wide_tail,
    }
}

/// The character that occupies the cell.
///
/// A cell can also carry combining marks after its base character; the base is
/// what the grid keeps, since that is the character the column is drawn for.
fn first_char(cell: &vt100::Cell) -> char {
    cell.contents().chars().next().unwrap_or(' ')
}

/// The cell's drawing attributes, in the grid's vocabulary.
fn style_of(cell: &vt100::Cell) -> Style {
    Style {
        fg: colour(cell.fgcolor()),
        bg: colour(cell.bgcolor()),
        bold: cell.bold(),
        italic: cell.italic(),
        underline: cell.underline(),
        reverse: cell.inverse(),
    }
}

/// A colour in the wire form's vocabulary.
fn colour(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Default,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_is_a_space_with_default_style() {
        let c = StyledCell::blank();
        assert_eq!(c.ch, ' ');
        assert_eq!(c.style, Style::default());
    }

    #[test]
    fn cells_compare_by_ch_and_style() {
        let mut a = StyledCell::new('a');
        let b = StyledCell::new('a');
        assert_eq!(a, b);
        a.style.bold = true;
        assert_ne!(a, b);
    }

    #[test]
    fn colours_roundtrip_through_their_wire_form() {
        // Every recorded frame carries these, so a colour that cannot be written
        // is a frame that cannot be recorded.
        for c in [Color::Default, Color::Indexed(17), Color::Rgb(1, 2, 3)] {
            let json = serde_json::to_string(&c).expect("colour serialises");
            let back: Color = serde_json::from_str(&json).expect("colour deserialises");
            assert_eq!(back, c, "{json}");
        }
        assert_eq!(
            serde_json::to_string(&Color::Default).unwrap(),
            r#"{"kind":"default"}"#
        );
        assert_eq!(
            serde_json::to_string(&Color::Indexed(17)).unwrap(),
            r#"{"kind":"indexed","value":17}"#
        );
        assert_eq!(
            serde_json::to_string(&Color::Rgb(1, 2, 3)).unwrap(),
            r#"{"kind":"rgb","value":[1,2,3]}"#
        );
    }
}

#[cfg(test)]
mod vt_tests {
    use super::*;
    use vt100::Parser;

    fn feed(bytes: &[u8], rows: u16, cols: u16) -> Grid {
        let mut p = Parser::new(rows, cols, 0);
        p.process(bytes);
        grid_from_screen(p.screen(), cols)
    }

    #[test]
    fn reads_characters_and_colours() {
        let g = feed(b"\x1b[31mR\x1b[0m.", 1, 4);
        assert_eq!(g[0][0].ch, 'R');
        assert_eq!(g[0][0].style.fg, Color::Indexed(1));
        assert_eq!(g[0][1].style.fg, Color::Default);
    }

    #[test]
    fn a_full_width_ideograph_occupies_two_columns_and_the_tail_is_a_continuation() {
        // Two CJK glyphs plus an ASCII letter on a 6-wide row. Each ideograph is
        // two columns of the emulator's accounting, so the letter must land at
        // column 4 — a one-char-per-column reader would put it at column 2.
        let g = feed("\u{4f60}\u{597d}x".as_bytes(), 1, 6);
        assert_eq!(g[0][0].ch, '\u{4f60}');
        assert!(g[0][1].wide_tail);
        assert_eq!(g[0][2].ch, '\u{597d}');
        assert!(g[0][3].wide_tail);
        assert_eq!(g[0][4].ch, 'x');
        assert_eq!(g[0].len(), 6);
    }

    #[test]
    fn rows_are_always_padded_to_the_declared_width() {
        let g = feed(b"hi", 3, 5);
        assert_eq!(g.len(), 3);
        assert!(g.iter().all(|r| r.len() == 5));
        assert_eq!(g[2][0].ch, ' ');
    }

    #[test]
    fn tabs_expand_to_the_next_stop_instead_of_being_dropped() {
        let g = feed(b"a\tb", 1, 10);
        assert_eq!(g[0][0].ch, 'a');
        assert_eq!(g[0][8].ch, 'b');
    }

    #[test]
    fn columns_beyond_the_screen_degrade_to_blanks() {
        // `cols` is the width the caller recorded. Asking for more columns than
        // the emulator has must widen the row with blanks, not panic: a frame is
        // always exactly the declared shape.
        let mut p = Parser::new(1, 3, 0);
        p.process(b"hi");
        let g = grid_from_screen(p.screen(), 6);
        assert_eq!(g[0].len(), 6);
        assert_eq!(g[0][1].ch, 'i');
        assert_eq!(g[0][5], StyledCell::blank());
    }
}
