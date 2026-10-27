//! The page model tested against a recorded session that is checked in.
//!
//! Every other real-data check in this repository is on demand: `ui-lm-capture audit`
//! and `page` are run by hand against a capture that then gets deleted. This fixture is
//! the opposite — two kilobytes of frame data from an actual recording, read by CI on
//! every push, so the model is tested against captured bytes rather than against frames
//! this crate constructed itself.
//!
//! The screen was drawn by a small program of our own, so the fixture carries no
//! third-party interface and no personal content. It was recorded on Windows through
//! ConPTY, which is why the source program writes its tool glyph as the CP437 byte
//! `0x04`: the pty host decodes an application's output as the OEM code page, so a
//! program that writes UTF-8 has its glyphs re-encoded (`♦` arrives as `ΓÖª`). The
//! meter's separator is plain ASCII for the same reason — the CP437 box-drawing byte
//! arrived as a replacement character. That is a property of the recording platform,
//! not of the model, and it is written down here because regenerating this fixture
//! without knowing it silently produces a screen whose tool line is not recognised.

use std::path::PathBuf;

use ui_lm_frames::read_frames;
use ui_lm_pages::{audit, page_from_frame, render_text};

/// The checked-in session.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("session")
}

#[test]
fn a_recorded_screen_yields_every_component_the_model_knows() {
    let (_, frames) = read_frames(&fixture()).expect("the fixture is a finished session");
    assert_eq!(frames.len(), 1, "the fixture holds one settled screen");

    let page = page_from_frame(&frames[0], "fixture");
    let kinds: Vec<&str> = page
        .create_surface
        .components
        .iter()
        .map(|component| component.component.as_str())
        .collect();
    assert_eq!(
        kinds,
        vec!["Column", "Block", "Block", "ToolCall", "Diff", "Meter"],
        "a recorded screen must produce every component kind this crate can name"
    );
}

#[test]
fn the_recorded_screen_round_trips_and_passes_its_audit() {
    let (_, frames) = read_frames(&fixture()).expect("the fixture is a finished session");
    let page = page_from_frame(&frames[0], "fixture");

    let problems = audit(&page);
    assert!(
        problems.is_empty(),
        "a recorded page must be sound: {problems:?}"
    );

    // The fixture's screen, exactly: the drawn text is the contract.
    let expected = vec![
        "Grok Build fixture".to_owned(),
        String::new(),
        "  > hello".to_owned(),
        String::new(),
        "  \u{2666} Read notes.txt".to_owned(),
        String::new(),
        "  - old line".to_owned(),
        "  + new line".to_owned(),
        String::new(),
        "1.2K / 500K | [Dashboard]".to_owned(),
    ];
    assert_eq!(render_text(&page), expected);
}
