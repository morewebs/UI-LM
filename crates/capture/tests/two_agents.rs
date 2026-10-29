//! The hardest recorded shape, built through the same entry the CLI uses.
//!
//! This exists because two rounds were spent guessing which branch of the reader builds a two-pane
//! page. The answer belongs in a test, where asking costs a second and the question is asked in the
//! same language as the fix.

use ui_lm_pages::page_from_frame;

fn frames(name: &str) -> Vec<ui_lm_frames::Frame> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../cases/hard")
        .join(name)
        .join("frames.jsonl");
    let text = std::fs::read_to_string(&path).expect("the recording is in the repository");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("a frame"))
        .collect()
}

#[test]
fn the_two_agent_recording_is_read_as_two_panes() {
    let frames = frames("wsl-two-agents-in-split");
    assert!(frames.len() > 40, "the recording has frames");
    let page = page_from_frame(&frames[40], "wsl-two-agents-in-split");
    assert_eq!(
        page.create_surface.panes.len(),
        2,
        "the hardest shape is two panes, and this is the test that says which branch builds it"
    );
    let left = &page.create_surface.panes[0];
    let right = &page.create_surface.panes[1];
    assert!(
        !left.is_empty() && !right.is_empty(),
        "both panes hold components"
    );
}
