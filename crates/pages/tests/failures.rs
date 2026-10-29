//! A program that fails, and the shape its failure is allowed to have.
//!
//! A shell's "not found" and a Python traceback are text: indented frames, caret markers, file names in
//! quotes. None of it is a tool call, a reasoning line or a meter, and a page that claimed otherwise would
//! describe structure the screen does not have. The reader gets this right; this test exists so that a
//! future rule about `•`, `♦` or box drawing cannot quietly annex it.

use ui_lm_pages::{page_from_text, Body, Envelope};

fn kinds(page: &Envelope) -> Vec<String> {
    page.create_surface
        .components
        .iter()
        .filter(|component| component.component != "Column")
        .map(|component| component.component.clone())
        .collect()
}

#[test]
fn a_traceback_is_one_block_and_nothing_else() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/failing-command.txt");
    let text = std::fs::read_to_string(path).expect("the fixture is in the repository");
    let page = page_from_text(&text, "test");
    assert_eq!(kinds(&page), vec!["Block".to_owned()], "a failure is text");
    let block = page
        .create_surface
        .components
        .iter()
        .find(|component| matches!(component.body, Body::Block { .. }))
        .expect("one block");
    if let Body::Block { lines, .. } = &block.body {
        assert!(
            lines.iter().any(|line| line.contains("Traceback")),
            "the words are kept"
        );
    }
}
