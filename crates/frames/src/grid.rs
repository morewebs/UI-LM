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
