//! The first golden: a known screen, and the page it must describe.
//!
//! The page model is the thing the model will be trained against, so it must not
//! change shape by accident. This test pins one screen's whole envelope â€” every id,
//! role, row span and provenance word â€” and compares it to a file in the repository.
//!
//! Regenerate the file deliberately, never to make a run pass:
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p ui-lm-pages --test golden
//! ```
//!
//! A diff in that file is a change to the IR, and it wants a reason in the commit
//! message. That is the whole value: the change becomes a decision again.

use std::path::PathBuf;

use ui_lm_pages::page_from_frame;

/// The screen the golden describes: a short exchange, a blank separator, and a
/// status line, which is the shape every recording of this application has shown.
const SCREEN: &[&str] = &[
    "Grok Build 1.0.70",
    "",
    "> what file is in this folder",
    "  This folder is empty.",
    "",
    "  Waiting for responseâ€¦ 0.4s",
];

/// Where the expected page lives.
fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("transcript.json")
}

#[test]
fn a_known_screen_describes_the_same_page_it_always_has() {
    let chars: Vec<Vec<char>> = SCREEN.iter().map(|line| line.chars().collect()).collect();
    let frame = ui_lm_frames::frame_from_chars(0, u16::try_from(SCREEN.len()).unwrap(), 60, &chars);
    let page = page_from_frame(&frame, "golden");
    let mut actual = serde_json::to_string_pretty(&page).unwrap();
    actual.push('\n');

    let path = golden_path();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "cannot read the golden at {}: {error}. Regenerate it with \
             UPDATE_GOLDEN=1 cargo test -p ui-lm-pages --test golden",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "the page for a known screen changed. If the change is intended, regenerate the \
         golden and say why in the commit message"
    );
}

#[test]
fn the_golden_is_not_empty() {
    // A golden regenerated from a broken code path would be a green test forever
    // after, so the file has to look like a page and not like an empty one.
    let text = std::fs::read_to_string(golden_path()).expect("the golden must exist");
    assert!(
        text.contains("\"catalogId\": \"moreweb.terminal/v1\""),
        "{text}"
    );
    assert!(text.contains("\"component\": \"Block\""), "{text}");
    assert!(text.contains("\"source\": \"seen\""), "{text}");
}
