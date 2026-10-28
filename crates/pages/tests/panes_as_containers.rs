//! The target for ADR 0004, written before the change.
//!
//! A split screen with structured content in one pane is the case the ADR was written for, and the
//! screen is recorded: an agent running in the left pane of a tmux split, holding a meter, a
//! reasoning line, tool calls and a composer, beside a shell on the right.
//!
//! Today the page holds every component in one flat list with nothing saying which side it came
//! from. These tests state what "done" means, and are ignored until the reader and the label path
//! split panes before they segment, in one change.

use ui_lm_pages::page_from_labels;

fn page_of_the_split() -> ui_lm_pages::Envelope {
    let screen = include_str!("fixtures/screens/agent-in-split-pane.txt");
    let text: Vec<String> = screen.lines().map(|line| line.to_owned()).collect();
    let labels = vec!["Block".to_owned(); text.len()];
    page_from_labels(&text, &labels, "agent-in-split")
}

#[test]
#[ignore = "ADR 0004: panes are not containers yet"]
fn the_split_screen_declares_two_panes() {
    let page = page_of_the_split();
    let panes = &page.create_surface.panes;
    assert_eq!(
        panes.len(),
        2,
        "the screen is split, so the page must say so"
    );
    for pane in panes {
        assert!(!pane.is_empty(), "a pane holds at least one component");
    }
}

#[test]
#[ignore = "ADR 0004: panes are not containers yet"]
fn every_component_belongs_to_exactly_one_pane() {
    let page = page_of_the_split();
    let panes = &page.create_surface.panes;
    assert_eq!(panes.len(), 2, "two panes to look through");
    let loose = page
        .create_surface
        .components
        .iter()
        .filter(|component| component.component != "Column")
        .filter(|component| !panes.iter().flatten().any(|id| *id == component.id))
        .count();
    assert_eq!(
        loose, 0,
        "no component may sit outside a pane on a split screen"
    );
    let sized: Vec<usize> = panes.iter().map(|pane| pane.len()).collect();
    assert!(
        sized.iter().all(|count| *count > 0),
        "both panes hold something: {sized:?}"
    );
}

#[test]
fn the_screen_still_describes_as_components() {
    // Passes today and must keep passing: the change is about grouping, not content.
    let page = page_of_the_split();
    let blocks = page
        .create_surface
        .components
        .iter()
        .filter(|component| component.component != "Column")
        .count();
    assert!(blocks > 0, "the screen describes as at least one component");
}
