//! A pane keeps its own colour.
//!
//! Two panes claim every row of the screen, so anything keyed by row gives one pane the other's
//! styling. The graft therefore copies panes by id, and this pins that: two panes with identical
//! rows and different styling must come through untouched.

use ui_lm_pages::{
    graft_spans, Body, Component, Emphasis, Envelope, InlineSpan, Provenance, Surface,
};

/// A page holding two panes, with the styling given for each.
fn page_with_panes(first: Vec<Vec<InlineSpan>>, second: Vec<Vec<InlineSpan>>) -> Envelope {
    let lines = || vec!["left pane text".to_owned(); 2];
    Envelope {
        version: "v0.9.1".to_owned(),
        create_surface: Surface {
            surface_id: "test".to_owned(),
            catalog_id: "moreweb.terminal/v1".to_owned(),
            root: "root".to_owned(),
            components: vec![
                Component {
                    id: "root".to_owned(),
                    component: "Column".to_owned(),
                    source: Provenance::Seen,
                    body: Body::Column {
                        children: vec!["pane-left".to_owned(), "pane-right".to_owned()],
                    },
                },
                Component {
                    id: "pane-left".to_owned(),
                    component: "Block".to_owned(),
                    source: Provenance::Seen,
                    body: Body::Block {
                        role: "pane".to_owned(),
                        rows: [0, 2],
                        lines: lines(),
                        spans: first,
                        tabs: Vec::new(),
                        shortcuts: Vec::new(),
                        properties: Vec::new(),
                    },
                },
                Component {
                    id: "pane-right".to_owned(),
                    component: "Block".to_owned(),
                    source: Provenance::Seen,
                    body: Body::Block {
                        role: "pane".to_owned(),
                        rows: [0, 2],
                        lines: lines(),
                        spans: second,
                        tabs: Vec::new(),
                        shortcuts: Vec::new(),
                        properties: Vec::new(),
                    },
                },
            ],
            panes: Vec::new(),
        },
    }
}

fn spans_of(page: &Envelope, id: &str) -> Vec<Vec<InlineSpan>> {
    page.create_surface
        .components
        .iter()
        .find(|component| component.id == id)
        .and_then(|component| match &component.body {
            Body::Block { spans, .. } => Some(spans.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn each_pane_keeps_its_own_styling_when_both_claim_every_row() {
    let bold = Emphasis {
        bold: true,
        ..Emphasis::default()
    };
    let italic = Emphasis {
        italic: true,
        ..Emphasis::default()
    };
    let reader = page_with_panes(
        vec![
            vec![InlineSpan {
                start: 0,
                end: 4,
                style: bold.clone(),
            }],
            Vec::new(),
        ],
        vec![
            vec![InlineSpan {
                start: 5,
                end: 9,
                style: italic.clone(),
            }],
            Vec::new(),
        ],
    );
    // What a page built from labels alone looks like: the same panes, no styling.
    let mut judged = page_with_panes(vec![Vec::new(); 2], vec![Vec::new(); 2]);

    graft_spans(&mut judged, &reader);

    assert_eq!(
        spans_of(&judged, "pane-left"),
        spans_of(&reader, "pane-left")
    );
    assert_eq!(
        spans_of(&judged, "pane-right"),
        spans_of(&reader, "pane-right")
    );
    assert_ne!(
        spans_of(&judged, "pane-left"),
        spans_of(&judged, "pane-right"),
        "the two panes must not end up with the same styling"
    );
}
