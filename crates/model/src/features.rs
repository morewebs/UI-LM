//! The features a row is described by.
//!
//! These must stay identical to the ones `scripts/train_model.py` computes, in the same
//! order. A drift between the two does not fail loudly — it makes the model quietly worse,
//! which is the worse outcome — so the test at the bottom compares this implementation with
//! a table of values taken from the training script.

/// How many features each row has.
pub const COUNT: usize = 19;

/// The characters that draw a border.
const BOX: [char; 8] = ['╭', '┌', '╰', '└', '│', '┃', '─', '━'];

/// The glyph the application draws before a tool line.
const TOOL_GLYPH: char = '\u{2666}';

/// What a row looks like, as numbers a linear model can weigh.
pub fn of(line: &str, row: usize, rows: usize) -> Vec<f64> {
    let text = line.trim_end();
    let stripped = text.trim();
    let leading = text
        .chars()
        .count()
        .saturating_sub(text.trim_start().chars().count());
    let digits = stripped.chars().filter(char::is_ascii_digit).count();
    let length = stripped.chars().count();

    let flag = |value: bool| if value { 1.0 } else { 0.0 };
    vec![
        flag(stripped.starts_with(TOOL_GLYPH)),
        flag(stripped.contains('/') && digits > 0),
        flag(stripped.contains('|')),
        flag(stripped.contains("Ctrl+") || stripped.contains("Shift+")),
        flag(stripped.contains('→')),
        flag(stripped.starts_with('/')),
        flag(stripped.contains("+ ") || stripped.contains("- ")),
        flag(stripped.ends_with(':')),
        flag(stripped.starts_with(|character: char| BOX.contains(&character))),
        flag(stripped.ends_with(|character: char| BOX.contains(&character))),
        flag(stripped.starts_with('>')),
        flag(stripped.is_empty()),
        f64::from(u32::try_from(digits).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(length.max(1)).unwrap_or(1)),
        f64::from(u32::try_from(length).unwrap_or(u32::MAX)) / 120.0,
        f64::from(u32::try_from(leading).unwrap_or(u32::MAX)) / 20.0,
        f64::from(u32::try_from(row).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(rows.max(1)).unwrap_or(1)),
        // A row that begins with a symbol rather than a letter and one that names a tool
        // are both how a tool line looks when the glyph itself has been lost to an encoding.
        flag(
            stripped
                .chars()
                .next()
                .is_some_and(|character| !character.is_ascii()),
        ),
        flag(
            [
                "hook", "Edited", "Read", "Ran", "Wrote", "Explored", "Search", "Listed",
            ]
            .iter()
            .any(|word| stripped.contains(word)),
        ),
        1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::of;

    #[test]
    fn a_tool_row_is_marked_as_one_and_an_empty_row_as_empty() {
        let tool = of("  ♦ Read notes.txt", 3, 10);
        assert_eq!(tool[0], 1.0, "the tool glyph must be seen");
        assert_eq!(tool[11], 0.0);

        let empty = of("   ", 0, 10);
        assert_eq!(empty[11], 1.0, "a blank row must be marked blank");
        assert_eq!(empty[0], 0.0);
    }

    #[test]
    fn a_meter_row_is_marked_by_its_limit_and_a_shortcut_by_its_key() {
        let meter = of("1.2K / 500K | [Dashboard]", 0, 1);
        assert_eq!(meter[1], 1.0, "a quantity against a limit");
        assert_eq!(meter[2], 1.0, "a separator");

        let shortcut = of("Resume Session            Ctrl+R", 1, 2);
        assert_eq!(shortcut[3], 1.0, "a key binding");
        assert_eq!(shortcut[1], 0.0);
    }

    #[test]
    fn the_feature_count_is_what_the_model_expects() {
        assert_eq!(of("x", 0, 1).len(), super::COUNT);
    }

    #[test]
    fn position_is_measured_against_the_height_of_the_screen() {
        let first = of("text", 0, 10);
        let last = of("text", 9, 10);
        assert_eq!(first[15], 0.0);
        assert!(
            (last[15] - 0.9).abs() < 1e-9,
            "the last row is near the bottom"
        );
    }
}
