//! Building a page from a guess about what each row is.
//!
//! The ordinary page is built from the screen alone: the reader decides what each row is by
//! looking at it. This is the same construction with that decision supplied from outside —
//! one label per row — so a model can make the judgement while the page is still built by
//! the code that knows how a component is shaped.
//!
//! Keeping the construction here matters. A model that had to invent a `Meter`'s fields or a
//! `ToolCall`'s would be a second, untested description of the catalogue, and the two would
//! drift apart within a week.
//!
//! A label the text does not support is ignored rather than honoured: if a row is called a
//! meter and no limit can be read from it, the row is ordinary text. A page must never claim
//! a component the screen cannot justify.

use crate::{
    as_meter, block_id, properties_of, role_of, shortcuts_of, tabs_of, thought_seconds, tool_calls,
    Body, Component, Envelope, Provenance, Surface, CATALOGUE_ID, ENVELOPE_VERSION,
};

/// Splits rows into runs of the same label, in the order drawn.
fn runs(labels: &[String]) -> Vec<(usize, usize, String)> {
    let mut found: Vec<(usize, usize, String)> = Vec::new();
    for (row, label) in labels.iter().enumerate() {
        match found.last_mut() {
            Some(last) if last.2 == *label => last.1 = row + 1,
            _ => found.push((row, row + 1, label.clone())),
        }
    }
    found
}

/// A page built from `labels`, one per row of `text`.
pub fn page_from_labels(text: &[String], labels: &[String], surface: &str) -> Envelope {
    let mut children = Vec::new();
    let mut components = Vec::new();

    for (first, end, label) in runs(labels) {
        if first >= text.len() {
            continue;
        }
        let end = end.min(text.len());
        let lines: Vec<String> = text[first..end].to_vec();
        let rows = [
            u16::try_from(first).unwrap_or(u16::MAX),
            u16::try_from(end).unwrap_or(u16::MAX),
        ];

        let (kind, body) = body_for(label.as_str(), &lines, rows);
        let id = block_id(&lines);
        children.push(id.clone());
        components.push(Component {
            id,
            component: kind,
            source: Provenance::Seen,
            body,
        });
    }

    let root = format!("{surface}-root");
    let mut all = vec![Component {
        id: root.clone(),
        component: "Column".to_owned(),
        source: Provenance::Seen,
        body: Body::Column { children },
    }];
    all.extend(components);
    Envelope {
        version: ENVELOPE_VERSION.to_owned(),
        create_surface: Surface {
            surface_id: surface.to_owned(),
            catalog_id: CATALOGUE_ID.to_owned(),
            root,
            components: all,
        },
    }
}

/// The component a run of rows becomes, given what it was called.
fn body_for(label: &str, lines: &[String], rows: [u16; 2]) -> (String, Body) {
    let text = lines.to_vec();
    match label {
        "ToolCall" => {
            let calls = tool_calls(&text);
            if calls.is_empty() {
                return as_text(text, rows);
            }
            // A reasoning line is a tool line whose name is Thought; the label comes from
            // the model but the shape is the same one the reader produces.
            if calls.len() == 1 && calls[0].tool.eq_ignore_ascii_case("thought") {
                return (
                    "Thought".to_owned(),
                    Body::Thought {
                        rows,
                        text: calls[0].text.trim_end().to_owned(),
                        seconds: thought_seconds(&calls[0].text),
                    },
                );
            }
            ("ToolCall".to_owned(), Body::ToolCall { rows, calls })
        }
        "Meter" => match as_meter(&text) {
            Some(meter) => (
                "Meter".to_owned(),
                Body::Meter {
                    rows,
                    used: meter.used,
                    limit: meter.limit,
                    fraction: meter.fraction,
                    label: meter.label,
                    text: meter.text,
                },
            ),
            None => as_text(text, rows),
        },
        _ => as_text(text, rows),
    }
}

/// A run of rows as an ordinary block, read the way the reader reads one.
fn as_text(lines: Vec<String>, rows: [u16; 2]) -> (String, Body) {
    let spans = vec![Vec::new(); lines.len()];
    let body = Body::Block {
        role: role_of(&lines).to_owned(),
        rows,
        tabs: tabs_of(&lines),
        shortcuts: shortcuts_of(&lines),
        properties: properties_of(&lines),
        lines,
        spans,
    };
    ("Block".to_owned(), body)
}
