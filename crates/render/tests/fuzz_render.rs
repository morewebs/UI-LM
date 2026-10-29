//! Any page at all: the renderers must not panic, and the document must say what the page says.
//!
//! The reader's fuzz test asks whether a nonsense screen can make a *page*. This asks the next
//! question: whether that page can be drawn — as text and as a document — and whether the document
//! still holds the page's own words.

use ui_lm_pages::{component_text, page_from_text, render_text};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

#[test]
fn no_page_makes_a_renderer_panic_or_drop_its_words() {
    let pieces = [
        " ",
        "│",
        "─",
        "┌",
        "┐",
        "└",
        "┘",
        "a",
        "Zebra",
        "0",
        "·",
        "●",
        "✗",
        "⎿",
        "日本語",
        "\t",
        "Read",
        "notes.txt",
        "+2 -2",
        "Thought",
        "  ",
    ];
    let mut rng = Rng(0xb0a7);
    let mut problems = Vec::new();

    for index in 0..300 {
        let rows = 1 + (rng.next() % 10) as usize;
        let mut screen = String::new();
        for row in 0..rows {
            if row > 0 {
                screen.push('\n');
            }
            for _ in 0..(rng.next() % 12) {
                screen.push_str(pieces[(rng.next() as usize) % pieces.len()]);
            }
        }
        let page = page_from_text(&screen, "fuzz");

        let drawn = render_text(&page);
        let _ = drawn.len();

        let document = ui_lm_render::render_page(&page, "fuzz");
        if !document.starts_with("<!doctype html>") {
            problems.push(format!("screen {index}: not a document: {screen:?}"));
        }
        for component in &page.create_surface.components {
            for text in component_text(&component.body) {
                for word in text.split(|c: char| !c.is_alphanumeric()) {
                    if word.chars().count() < 4 {
                        continue;
                    }
                    if !document.contains(word) {
                        problems.push(format!(
                            "screen {index}: {word:?} reached the page but not the document"
                        ));
                    }
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "{} page(s) drew badly:\n{}",
        problems.len(),
        problems
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
