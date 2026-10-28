//! What the model's page declares for a screen the gate calls split.

use std::path::PathBuf;

use ui_lm_model::Model;
use ui_lm_pages::{panes_hold_structure, split_panes};

fn model() -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/page.json");
    Model::load(&path).expect("the shipped model must load")
}

fn frame_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../pages/tests/fixtures/screens/grok-help-frame6.txt");
    std::fs::read_to_string(path).expect("the exported frame must exist")
}

#[test]
fn the_gate_and_the_model_agree_about_splitting() {
    let text = frame_text();
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    assert!(
        split_panes(&lines).is_some(),
        "the fixture must hold a divider"
    );

    let page = model().page(&text, "grok-help");
    let declared = page.create_surface.panes.len();
    let gate = panes_hold_structure(&lines);
    assert_eq!(declared > 0, gate, "gate={gate} declared={declared}");
}
