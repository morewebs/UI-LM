//! Any screen at all, through the learned path.
//!
//! The model is the component most likely to meet something it has never seen — it is a classifier
//! trained on one application's screens, and the recorder is pointed at whatever a program draws. This
//! walks the same spread the reader's fuzz test uses and asks the two questions that matter for a page
//! a renderer will later be handed: it returns, and `audit` accepts what it returned.

use ui_lm_model::Model;
use ui_lm_pages::audit;

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

fn model() -> Model {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/page.json");
    Model::load(&path).expect("the trained weights are in the repository")
}

#[test]
fn no_screen_makes_the_model_panic_or_make_a_bad_page() {
    let trained = model();
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
        "25K / 500K",
        "● Running 3 tools…",
    ];
    let mut rng = Rng(0x3d1e);
    let mut problems = Vec::new();
    let mut pages = 0;

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
        let page = trained.page(&screen, "fuzz");
        pages += 1;
        for problem in audit(&page) {
            problems.push(format!("screen {index}: {problem}\n  {screen:?}"));
        }
    }

    assert!(pages == 300, "every screen was read");
    assert!(
        problems.is_empty(),
        "{} page(s) from the model fail the audit:\n{}",
        problems.len(),
        problems
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
