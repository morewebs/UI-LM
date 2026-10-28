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
    as_diff, as_meter, block_id, properties_of, role_of, shortcuts_of, tabs_of, thought_seconds,
    tool_calls, Body, Component, Envelope, Provenance, Surface, CATALOGUE_ID, ENVELOPE_VERSION,
};

/// Splits rows into runs of the same label, in the order drawn.
fn runs(labels: &[String], text: &[String]) -> Vec<(usize, usize, String)> {
    let mut last_closed = false;
    let mut found: Vec<(usize, usize, String)> = Vec::new();
    for (row, label) in labels.iter().enumerate() {
        // A blank row ends a run: it is the reader's own separator, and a model that
        // merged across every blank line would describe a whole screen as one block —
        // measured on 78 blocks, it matched 11.
        let blank = text
            .get(row)
            .map(|line| line.trim().is_empty())
            .unwrap_or(false);
        // Only a border that opens or closes a box ends a run. A vertical bar is the side
        // of a box, not a boundary between two things: splitting on it turned an eight-row
        // panel into eight one-row blocks, which is why blocks were the worst component in
        // the score while everything else was perfect.
        let opens = text
            .get(row)
            .map(|line| line.trim().starts_with(['╭', '┌']))
            .unwrap_or(false);
        let closes = text
            .get(row)
            .map(|line| line.trim().starts_with(['╰', '└']))
            .unwrap_or(false);
        // Narrow on purpose: only a tree branch, `└── name`. A wider reading — "a closing
        // glyph with any text after it" — also caught the application's own box, whose
        // closing rule carries a label (`╰── Grok 4.7 (low) ─╯`), and took the training
        // batch from 100% to 90.5%. A rule written for one screen shape must be as narrow as
        // that shape, or it rewrites the screens that were already right.
        let named = text
            .get(row)
            .map(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("└──")
            })
            .unwrap_or(false);
        let starts = opens || named;
        match found.last_mut() {
            Some(last) if last.2 == *label && !blank && !starts && !last_closed => last.1 = row + 1,
            _ => found.push((row, row + 1, label.clone())),
        }
        last_closed = blank || closes;
    }
    found
}

/// Whether a row is shaped like a meter: a use, a slash, a limit.
///
/// A slash alone is not enough. `> /import-memory` holds one and is the application's input
/// line; labelled a meter it split the pane it sits in from every row below it — twelve
/// blocks wrong on one screen, the last structural gap the model had. A meter starts with a
/// number, because a meter is a quantity used against a quantity allowed.
fn looks_like_meter(line: &str) -> bool {
    let mut words = line.split_whitespace();
    let used = words.next().unwrap_or("");
    let number = used.chars().any(|ch| ch.is_ascii_digit());
    let slash = line.split_whitespace().any(|word| word == "/");
    number && slash && !line.contains(';')
}

/// Labels with the per-row evidence applied, before any grouping.
///
/// This has to run before `runs` rather than after: a label decides where one block ends and
/// the next begins, so a label corrected after grouping cannot put a split block back
/// together. Two of the three guards are therefore per row — a row that opens with a list
/// glyph is not a thought, and a row that is not shaped like a use and a limit is not a
/// meter — while the diff guard needs the whole run and stays in `settled`.
fn adjusted(labels: &[String], text: &[String]) -> Vec<String> {
    labels
        .iter()
        .enumerate()
        .map(|(row, label)| {
            let line = text.get(row).map(|line| line.trim()).unwrap_or("");
            match label.as_str() {
                // A row can only open a diff if it is drawn as one. `-- (empty)` from a
                // package listing is not a removal, and calling it one split a two-row
                // screen into two blocks — the same lesson as the meter guard: a label that
                // decides grouping has to be right before the grouping happens.
                "Diff" if !(line.starts_with("+ ") || line.starts_with("- ")) => "Block".to_owned(),
                // A thought is a row the application marks as one: it carries the mark and
                // says so — `♦ Thought for 0.6s`. Anything else that merely starts with a
                // non-ASCII glyph is not a thought, and a table whose status column uses
                // `✓` shattered into seven blocks because each row was called one.
                "Thought" if !(line.contains("Thought") && line.starts_with('♦')) => {
                    "Block".to_owned()
                }
                "Thought" if line.starts_with(['○', '•', '●', '·', '▪', '◦']) => {
                    "Block".to_owned()
                }
                // A tool call is marked by the mark the application draws for it. A row that
                // merely begins with a verb is prose: `Read notes.txt first line` is the
                // user's own message echoed back, and calling it a tool call cut a block in
                // two on the one screen where it appeared.
                "ToolCall" if !line.starts_with('♦') => "Block".to_owned(),
                "Meter" if !looks_like_meter(line) => "Block".to_owned(),
                other => other.to_owned(),
            }
        })
        .collect()
}

/// The label a run of rows can actually support.
///
/// One piece of evidence is only available once the rows are grouped: a lone `+ New Agent` row
/// is a button, not a diff, because a diff is a change and one line is not a change of
/// anything. A diff therefore needs at least two changed rows.
///
/// These are the same structural checks the reader makes, applied to a label that came from
/// somewhere else, so a model can be wrong about a row and the page still not claim something
/// the screen cannot justify.
fn settled(label: &str, lines: &[String]) -> String {
    let changed = lines
        .iter()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("+ ") || line.starts_with("- ")
        })
        .count();
    match label {
        "Diff" if changed < 2 => "Block".to_owned(),
        other => other.to_owned(),
    }
}

/// A page built from `labels`, one per row of `text`.
pub fn page_from_labels(text: &[String], labels: &[String], surface: &str) -> Envelope {
    let mut children = Vec::new();
    let mut components = Vec::new();

    for (first, end, label) in runs(&adjusted(labels, text), text) {
        if first >= text.len() {
            continue;
        }
        let end = end.min(text.len());
        let lines: Vec<String> = text[first..end].to_vec();
        // A run of nothing is not a component: a separator row has no content to describe.
        if lines.iter().all(|line| line.trim().is_empty()) {
            continue;
        }
        let rows = [
            u16::try_from(first).unwrap_or(u16::MAX),
            u16::try_from(end).unwrap_or(u16::MAX),
        ];

        let (kind, body) = body_for(settled(&label, &lines).as_str(), &lines, rows);
        let id = block_id(&lines);
        children.push(id.clone());
        components.push(Component {
            id,
            component: kind,
            source: Provenance::Seen,
            body,
        });
    }

    // A screen the reader splits into panes must split here too. The label path is meant to
    // reproduce the reader's page given the reader's judgement, so a shape found by the
    // reader and not by this path is the model losing a component it was never shown — the
    // measured cost of that gap was a model tying the floor on every split screen.
    if components.len() == 1 {
        if let Some((left, right)) = crate::as_panes(text) {
            let first_id = block_id(&left);
            let second_id = block_id(&right);
            if first_id != second_id {
                let pane = |id: String, lines: Vec<String>| Component {
                    id,
                    component: "Block".to_owned(),
                    source: Provenance::Seen,
                    body: Body::Block {
                        role: "pane".to_owned(),
                        rows: [0, u16::try_from(lines.len()).unwrap_or(u16::MAX)],
                        tabs: Vec::new(),
                        shortcuts: Vec::new(),
                        properties: Vec::new(),
                        spans: vec![Vec::new(); lines.len()],
                        lines,
                    },
                };
                children = vec![first_id.clone(), second_id.clone()];
                components = vec![pane(first_id, left), pane(second_id, right)];
            }
        }
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
    // A reasoning line says what it is: the application draws `♦ Thought for 3.0s`, and the
    // reader has always recognised it from the text rather than from a decision. The model
    // deciding "tool call" for such a row must not turn it into one, or the page claims the
    // reasoning was a tool. Measured: 0 of 17 reasoning lines reproduced while this rule
    // depended on the model's label.
    if text.len() == 1 {
        let calls = tool_calls(&text);
        if calls.len() == 1 && calls[0].tool.eq_ignore_ascii_case("thought") {
            return (
                "Thought".to_owned(),
                Body::Thought {
                    rows,
                    text: calls[0].text.clone(),
                    seconds: thought_seconds(&calls[0].text),
                },
            );
        }
    }
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
        // A diff is a run of rows that say what was removed and what was added. The model
        // may call them that; the component is still only built if the rows really are a
        // diff, because a page must not claim what the screen cannot justify.
        "Diff" => {
            let spans = vec![Vec::new(); text.len()];
            match as_diff(&text, &spans) {
                Some(lines) => ("Diff".to_owned(), Body::Diff { rows, lines }),
                None => as_text(text, rows),
            }
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
    // "Ordinary block" means what the reader means by it: the reader asks whether a run
    // holds a removal and an addition, not what each row was called. A real `git diff` is
    // ten rows whose first two are `diff --git` and `index`, and a model reading them one at
    // a time calls most of them text — so the run is offered to the diff reader as well.
    // Measured on a real `git diff`: 0% before this, 100% after.
    if let Some(diff) = as_diff(&lines, &spans) {
        return ("Diff".to_owned(), Body::Diff { rows, lines: diff });
    }
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
#[cfg(test)]
mod tests {
    use super::page_from_labels;

    fn labels(count: usize, kind: &str) -> Vec<String> {
        vec![kind.to_owned(); count]
    }

    #[test]
    fn a_blank_row_ends_a_block() {
        // The mistake that cost 33 blocks out of 78: every text row merged into one
        // component, because only the label was consulted and every row was text.
        let text: Vec<String> = ["first paragraph", "", "second paragraph"]
            .iter()
            .map(|line| (*line).to_owned())
            .collect();
        let page = page_from_labels(&text, &labels(3, "Block"), "s");
        let blocks = page
            .create_surface
            .components
            .iter()
            .filter(|component| component.component == "Block")
            .count();
        assert_eq!(blocks, 2, "a blank line separates two paragraphs");
    }

    #[test]
    fn a_border_ends_a_block() {
        let text: Vec<String> = ["╭──╮", "│ x│", "╰──╯", "after"]
            .iter()
            .map(|line| (*line).to_owned())
            .collect();
        let page = page_from_labels(&text, &labels(4, "Block"), "s");
        let blocks = page
            .create_surface
            .components
            .iter()
            .filter(|component| component.component == "Block")
            .count();
        assert!(
            blocks >= 2,
            "a bordered box is its own thing: {blocks} block(s)"
        );
    }

    #[test]
    fn a_label_the_text_does_not_support_falls_back_to_text() {
        // A model may say "meter" about a row with no limit in it. The page must not claim
        // a component the screen cannot justify.
        let text = vec!["just some words".to_owned()];
        let page = page_from_labels(&text, &labels(1, "Meter"), "s");
        assert!(
            page.create_surface
                .components
                .iter()
                .any(|component| component.component == "Block"),
            "an unsupported meter must become text: {page:?}"
        );
    }
}
