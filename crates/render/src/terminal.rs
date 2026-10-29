//! Drawing a page back into a terminal.
//!
//! The page is the middle of this project: a screen is read into it, and a runtime draws
//! it. The browser is one such runtime. This is the other direction — a page drawn as the
//! text a terminal would show — and it exists so the round trip can be measured instead of
//! assumed.
//!
//! It is deliberately rule-based. Every component in the page carries the text it was read
//! from, so drawing it back is transcription rather than generation, and a model would only
//! add a way to be wrong. What a page cannot do is recover what it never recorded: the page
//! holds a thought's duration but not the reasoning that was collapsed, so the drawn screen
//! is faithful only where the model of the screen is faithful.

use ui_lm_pages::{Body, Component, Envelope};

/// One component's rows, in the order a terminal would show them.
fn rows_of(component: &Component) -> Vec<String> {
    match &component.body {
        Body::Column { children } => children.clone(),
        Body::Block { lines, .. } => lines.clone(),
        Body::Diff { lines, .. } => lines.iter().map(|line| line.text.clone()).collect(),
        Body::ToolCall { calls, .. } => calls.iter().map(|call| call.text.clone()).collect(),
        Body::Thought { text, .. } => vec![text.clone()],
        Body::Meter { text, .. } => vec![text.clone()],
    }
}

/// The whole page as terminal text, one component after another.
///
/// Components are joined the way the reader found them: consecutive, with no invented blank
/// line between them, because a blank line the screen did not have is a row the round trip
/// would have to explain.
pub fn render_terminal(page: &Envelope) -> String {
    let components = &page.create_surface.components;
    // The column names its children; a page with no such root falls back to drawing every
    // component in the order given, which is the order the reader read them.
    let order: Vec<String> = components
        .iter()
        .find_map(|component| match &component.body {
            Body::Column { children } => Some(children.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            components
                .iter()
                .filter(|component| !matches!(component.body, Body::Column { .. }))
                .map(|component| component.id.clone())
                .collect()
        });

    // Two panes are drawn side by side, because that is what a split screen is. Drawing them
    // one after another turned a 100% round trip into 32.6%: the content was all there, and
    // every row of it was in the wrong place.
    //
    // The page says which components share the screen. The older `pane` role is still accepted so
    // a page recorded before the grouping existed can still be drawn.
    let groups: Vec<Vec<String>> = if page.create_surface.panes.is_empty() {
        components
            .iter()
            .filter(
                |component| matches!(&component.body, Body::Block { role, .. } if role == "pane"),
            )
            .map(|component| vec![component.id.clone()])
            .collect()
    } else {
        page.create_surface.panes.clone()
    };
    // A paned page is drawn by the reader's own renderer: it places each component at the row it
    // claims, which this function could only do by duplicating that logic. Stacking components
    // instead lost the rows — two full panes came back at 23.8%, and the same page drawn by
    // `ui_lm_pages::render_text` matched the screen line for line.
    if !page.create_surface.panes.is_empty() {
        return ui_lm_pages::render_text(page).join("\n");
    }
    if groups.len() == 2 {
        let pane_rows = |ids: &[String]| -> Vec<String> {
            let mut out = Vec::new();
            for id in ids {
                if let Some(component) = components.iter().find(|component| component.id == *id) {
                    out.extend(rows_of(component));
                }
            }
            out
        };
        let left = pane_rows(&groups[0]);
        let right = pane_rows(&groups[1]);
        let width = left
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        let mut merged: Vec<String> = Vec::new();
        for row in 0..left.len().max(right.len()) {
            let first = left.get(row).cloned().unwrap_or_default();
            let padding = " ".repeat(width.saturating_sub(first.chars().count()));
            let second = right.get(row).cloned().unwrap_or_default();
            // Trailing space is not part of what a row says: a short row padded to the pane
            // width must come back as the short row it was.
            merged.push(format!("{first}{padding}{second}").trim_end().to_owned());
        }
        return merged.join("\n");
    }

    let mut lines: Vec<String> = Vec::new();
    for id in order {
        if let Some(component) = components.iter().find(|component| component.id == id) {
            lines.extend(rows_of(component));
        }
    }
    lines.join("\n")
}

/// How much of a screen a page can draw back, row by row.
///
/// Two numbers, because they fail differently: a row that is present but shifted means the
/// page remembered the text and lost the position, while a row that is missing means the
/// page never held it.
pub struct RoundTrip {
    /// Rows of the original screen that held something.
    pub content: usize,
    /// How many of those came back, in the order they were read.
    pub content_same: usize,
    /// Rows in the original screen, padding included.
    pub rows: usize,
    /// Rows identical at the same position, padding included.
    pub same: usize,
    /// Rows the page drew.
    pub drawn: usize,
}

impl RoundTrip {
    /// The share of the screen's content that came back.
    pub fn content_fraction(&self) -> f64 {
        if self.content == 0 {
            return 1.0;
        }
        self.content_same as f64 / self.content as f64
    }

    /// The share of the whole screen, padding included, that came back in place.
    pub fn layout_fraction(&self) -> f64 {
        if self.rows == 0 {
            return 1.0;
        }
        self.same as f64 / self.rows as f64
    }
}

/// Draws a page back and compares it to the screen it was read from.
///
/// Two measures, because they answer different questions and one of them is nearly useless
/// alone. **Content** asks whether the page remembers what the screen said. **Layout** asks
/// whether it remembers where — and a page cannot, because a blank row is a separator the
/// reader consumes rather than records, while `frame_text` keeps every one. A screen that is
/// mostly padding therefore scores low on layout however faithfully it is described.
pub fn round_trip(page: &Envelope, screen: &str) -> RoundTrip {
    let drawn = render_terminal(page);
    let original: Vec<&str> = screen.lines().map(|line| line.trim_end()).collect();
    let packed = drawn.split('\n').collect::<Vec<_>>();
    let same = original
        .iter()
        .zip(packed.iter())
        .filter(|(left, right)| left == right)
        .count();

    // Rows with something in them, matched in order: a page that reordered the screen's
    // content would not be faithful, so the walk is sequential rather than a set count.
    let content: Vec<&str> = original
        .iter()
        .copied()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let mut cursor = 0usize;
    let mut content_same = 0usize;
    for row in &content {
        while cursor < packed.len() {
            let candidate = packed[cursor];
            cursor += 1;
            if candidate == *row {
                content_same += 1;
                break;
            }
        }
    }
    RoundTrip {
        content: content.len(),
        content_same,
        rows: original.len(),
        same,
        drawn: packed.len(),
    }
}
