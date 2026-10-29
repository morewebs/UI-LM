//! Any screen at all: the reader must not panic, and the page it makes must be a page.
//!
//! Every other test here is given a screen that means something. The recorder runs next to programs
//! nobody chose — a build log, a stack trace, a screen full of box characters — and a page built from
//! one of those is still a page a renderer will be handed. This walks a deterministic spread of shapes
//! (empty, one cell, borders, ragged indentation, unicode, long rows) plus a few hundred pseudo-random
//! ones and asks two things of each: it returns, and `audit` finds nothing wrong with what it returned.

use ui_lm_pages::{audit, page_from_text};

/// A small deterministic generator, so a failure can be reproduced from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn pick(&mut self, options: &[&str]) -> String {
        options[(self.next() as usize) % options.len()].to_owned()
    }
}

fn shapes() -> Vec<String> {
    vec![
        String::new(),
        " ".to_owned(),
        "x".to_owned(),
        "\n\n\n".to_owned(),
        "┌───┐\n│ a │\n└───┘".to_owned(),
        "╭────╮\n│ ok │\n╰────╯".to_owned(),
        "  indented\n    deeper\nnone".to_owned(),
        "日本語のテキスト\nemoji ✅ done".to_owned(),
        "a".repeat(500),
        (0..60)
            .map(|row| format!("row {row}"))
            .collect::<Vec<_>>()
            .join("\n"),
        "│ left │ right │".to_owned(),
        "┌────┬────┐\n│ a  │ b  │\n└────┴────┘".to_owned(),
        "● Running 5 tools…\n✗ failed".to_owned(),
        "  thought for 3s\n  · considering".to_owned(),
        "⎿ Read notes.txt (+2 -2)".to_owned(),
        "\u{0}\u{1}\u{7}".to_owned(),
    ]
}

#[test]
fn no_screen_makes_the_reader_panic_or_make_a_bad_page() {
    let mut all = shapes();
    let mut rng = Rng(0x5eed);
    let pieces = [
        " ",
        "│",
        "─",
        "┌",
        "┐",
        "└",
        "┘",
        "├",
        "┤",
        "a",
        "Z",
        "0",
        "9",
        "·",
        "●",
        "✗",
        "⎿",
        "日本語",
        "✅",
        "\t",
        "  ",
        "…",
        "+2",
        "-1",
        "Read",
        "Thought",
    ];
    for _ in 0..400 {
        let rows = 1 + (rng.next() % 12) as usize;
        let mut screen = String::new();
        for row in 0..rows {
            if row > 0 {
                screen.push('\n');
            }
            let cells = rng.next() % 14;
            for _ in 0..cells {
                screen.push_str(&rng.pick(&pieces));
            }
        }
        all.push(screen);
    }

    let mut problems = Vec::new();
    for (index, screen) in all.iter().enumerate() {
        let page = page_from_text(screen, "fuzz");
        for problem in audit(&page) {
            problems.push(format!("screen {index}: {problem:?}\n  {screen:?}"));
        }
    }
    assert!(
        problems.is_empty(),
        "{} screen(s) produced a page the audit refuses:\n{}",
        problems.len(),
        problems
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
