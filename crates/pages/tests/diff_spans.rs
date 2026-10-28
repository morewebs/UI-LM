//! A diff keeps its colour.
//!
//! The span graft handled blocks and skipped diffs, so a model-built diff carried the right text,
//! the right kind and the right role, and no styling at all. `compare` never looks at spans, so the
//! score stayed at 100% while every diff on the page would have drawn in one colour. This pins the
//! fix: a diff's lines keep the styling the screen drew.

use ui_lm_pages::{
    graft_spans, Body, Component, DiffLine, Emphasis, Envelope, InlineSpan, Provenance, Surface,
};

fn page_with_diff(spans: Vec<InlineSpan>) -> Envelope {
    let line = |text: &str, spans: Vec<InlineSpan>| DiffLine {
        kind: if text.starts_with('+') {
            "add".to_owned()
        } else {
            "remove".to_owned()
        },
        text: text.to_owned(),
        spans,
    };
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
                        children: vec!["diff".to_owned()],
                    },
                },
                Component {
                    id: "diff".to_owned(),
                    component: "Diff".to_owned(),
                    source: Provenance::Seen,
                    body: Body::Diff {
                        rows: [0, 2],
                        lines: vec![
                            line("-apples 12", vec![spans[0].clone()]),
                            line("+apples 14", vec![spans[1].clone()]),
                        ],
                    },
                },
            ],
        },
    }
}

fn spans_of(page: &Envelope) -> Vec<Vec<InlineSpan>> {
    page.create_surface
        .components
        .iter()
        .find_map(|component| match &component.body {
            Body::Diff { lines, .. } => Some(lines.iter().map(|line| line.spans.clone()).collect()),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn a_judged_diff_receives_the_styling_the_screen_drew() {
    let removed = Emphasis {
        fg: Some("rgb:200,60,60".to_owned()),
        ..Emphasis::default()
    };
    let added = Emphasis {
        fg: Some("rgb:60,200,60".to_owned()),
        ..Emphasis::default()
    };
    let reader = page_with_diff(vec![
        InlineSpan {
            start: 0,
            end: 9,
            style: removed.clone(),
        },
        InlineSpan {
            start: 0,
            end: 9,
            style: added.clone(),
        },
    ]);
    // What a page built from labels alone looks like: the same lines, no styling.
    let mut judged = page_with_diff(vec![
        InlineSpan {
            start: 0,
            end: 0,
            style: Emphasis::default(),
        },
        InlineSpan {
            start: 0,
            end: 0,
            style: Emphasis::default(),
        },
    ]);

    graft_spans(&mut judged, &reader);

    let styled = spans_of(&judged);
    assert_eq!(styled.len(), 2);
    assert_eq!(
        styled[0][0].style, removed,
        "the removal keeps its own colour"
    );
    assert_eq!(
        styled[1][0].style, added,
        "the addition keeps its own colour"
    );
    assert_ne!(
        styled[0][0].style, styled[1][0].style,
        "and they are not the same colour"
    );
}
