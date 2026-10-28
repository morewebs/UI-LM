//! The split decision must be the same one in both paths.
//!
//! `panes_hold_structure` decides whether a screen is read pane by pane. The reader applies it in
//! `page_from_lines`; the label path applies it in `labeled_page`. If it says yes, **both** must
//! declare panes — a screen that splits in one path and not the other cannot agree about components,
//! and that is what three fresh Grok recordings have been telling us.
//!
//! The fixture is frame 6 of `grok-help`: 35 rows, one use-and-limit row, and a reader that splits
//! into two panes while building no meter at all.

use ui_lm_pages::{page_from_labels, panes_hold_structure, split_panes};

fn frame_lines() -> Vec<String> {
    include_str!("fixtures/screens/grok-help-frame6.txt")
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_fixture_is_the_hard_case() {
    let lines = frame_lines();
    assert!(lines.len() >= 30, "the fixture must be the whole screen");
    assert!(split_panes(&lines).is_some(), "it must hold a divider");
}

#[test]
fn the_label_path_splits_whatever_the_gate_says() {
    let lines = frame_lines();
    let labels = vec!["Block".to_owned(); lines.len()];
    let page = page_from_labels(&lines, &labels, "grok-help");
    let declared = page.create_surface.panes.len();

    if panes_hold_structure(&lines) {
        assert_eq!(
            declared, 2,
            "the gate says this screen is read pane by pane, so the label path must declare panes"
        );
    } else {
        assert_eq!(declared, 0, "the gate said no, so no panes may be declared");
    }
}
