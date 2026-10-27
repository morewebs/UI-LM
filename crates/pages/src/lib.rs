//! The page a recorded screen describes: UI-LM's first intermediate representation.
//!
//! A frame is a grid of styled cells — everything the terminal actually shows, and
//! nothing about what it *means*. This crate is the first step from the grid towards
//! meaning: it reads a frame and writes an A2UI v0.9.1 `createSurface` message whose
//! components come from the `moreweb.terminal/v1` catalogue (ADR 0002).
//!
//! ## What this first slice does
//!
//! It finds **blocks** — maximal runs of non-blank rows — and emits one `Block`
//! component per block: its row span, its lines of text, and a role. That is
//! deliberately the least interesting thing a page can be, and it is still the
//! first time a frame has been described rather than copied.
//!
//! ## What it does not do, and says so in the output
//!
//! - No `Table`, `Meter`, `ToolCall` or `Thought`: those need roles this slice
//!   cannot yet infer honestly.
//! - `Diff` is recognised from a block carrying both a `+` line and a `-` line, and
//!   that shape has not yet been seen on a real screen. Every recording so far ran
//!   with the application's auto-approval on, where an edit is reported in a
//!   sentence ("the first line of `notes.txt` now says …") and the diff card that
//!   would carry `+`/`-` lines is never drawn. So the component is tested against
//!   the shape, not against a captured screen; the corpus needs a run in a mode that
//!   asks before editing to prove it.
//! - No actions. Every keybinding is absent rather than guessed.
//! - No `guessed` or `carried` provenance: every component here is `seen`, because
//!   every character of it was on the screen. A later slice that reconstructs text
//!   from context must label that `guessed` and this crate gives it the vocabulary.
//!
//! ## Stable ids
//!
//! A component's id is derived from its own text, not from its position, because the
//! plan requires ids that survive a scroll (D15, and metric M5 is impossible
//! without them). A block that is still on screen after scrolling keeps its id.
//!
//! ## Scoring a page against another
//!
//! [`compare`] is the evaluation primitive: it scores one page against a reference and
//! reports what matched, separately for structure and for text. It exists so that a
//! future predictor has something to be measured with, and so that today's model has a
//! reference to score against — an identical page scores 1.0, and a page that splits a
//! block in two loses structural credit while keeping every character.

use std::collections::HashSet;

use serde::Serialize;
use ui_lm_frames::{decode_grid, Color, Frame, Style, StyledCell};

/// The envelope version this crate writes.
///
/// Pinned independently of the catalogue version: A2UI is pre-1.0, and an envelope
/// bump must not silently move the terminal vocabulary (ADR 0002).
pub const ENVELOPE_VERSION: &str = "v0.9.1";

/// The catalogue the components this crate writes are drawn from.
pub const CATALOGUE_ID: &str = "moreweb.terminal/v1";

/// Every component name `moreweb.terminal/v1` defines so far.
///
/// A page naming anything else describes something no renderer of this catalogue can
/// draw, so it is a fault rather than an extension — an unknown component is what the
/// degradation rule in the spec handles at *render* time, and a page this crate emits
/// has no business relying on it.
pub const COMPONENTS: [&str; 6] = ["Column", "Block", "ToolCall", "Meter", "Diff", "Thought"];

/// What is wrong with a page, in terms of the promises the page model makes.
///
/// The same idea as the frame audit: the model claims its components are reachable
/// from the root, uniquely identified, inside the screen they describe, and drawn
/// from one catalogue. All of that is checkable from the page alone, and until this
/// existed it had only been asserted by the code that produces it.
pub fn audit(page: &Envelope) -> Vec<String> {
    let mut problems = Vec::new();
    let surface = &page.create_surface;
    if page.version != ENVELOPE_VERSION {
        problems.push(format!(
            "envelope version is {:?}, not {ENVELOPE_VERSION:?}",
            page.version
        ));
    }
    if surface.catalog_id != CATALOGUE_ID {
        problems.push(format!(
            "catalogue is {:?}, not {CATALOGUE_ID:?}",
            surface.catalog_id
        ));
    }

    let mut ids = HashSet::new();
    for component in &surface.components {
        if !ids.insert(component.id.as_str()) {
            problems.push(format!("two components share the id {:?}", component.id));
        }
        if !COMPONENTS.contains(&component.component.as_str()) {
            problems.push(format!(
                "component {:?} names {:?}, which the catalogue does not define",
                component.id, component.component
            ));
        }
        match &component.body {
            Body::Column { children } => {
                for child in children {
                    if !surface
                        .components
                        .iter()
                        .any(|component| &component.id == child)
                    {
                        problems.push(format!(
                            "component {:?} lists the child {child:?}, which is not in the page",
                            component.id
                        ));
                    }
                }
            }
            Body::Block {
                lines, spans, rows, ..
            } => {
                if lines.len() != spans.len() {
                    problems.push(format!(
                        "component {:?} has {} line(s) and {} span list(s)",
                        component.id,
                        lines.len(),
                        spans.len()
                    ));
                }
                for (line, line_spans) in lines.iter().zip(spans) {
                    let width = line.chars().count();
                    for span in line_spans {
                        if span.start >= span.end || usize::from(span.end) > width {
                            problems.push(format!(
                                "component {:?} has a span {}..{} in a line of {width} \
                                 character(s)",
                                component.id, span.start, span.end
                            ));
                        }
                    }
                }
                check_rows(&mut problems, &component.id, rows);
            }
            Body::Diff { lines, rows } => {
                for line in lines {
                    let width = line.text.chars().count();
                    for span in &line.spans {
                        if span.start >= span.end || usize::from(span.end) > width {
                            problems.push(format!(
                                "component {:?} has a span {}..{} in a diff line of {width} \
                                 character(s)",
                                component.id, span.start, span.end
                            ));
                        }
                    }
                }
                check_rows(&mut problems, &component.id, rows);
            }
            Body::ToolCall { rows, .. } | Body::Meter { rows, .. } | Body::Thought { rows, .. } => {
                check_rows(&mut problems, &component.id, rows);
            }
        }
    }

    if !surface.components.iter().any(|c| c.id == surface.root) {
        problems.push(format!(
            "the root {:?} is not one of the page's components",
            surface.root
        ));
    }
    problems
}

/// Records a row range that covers nothing, which no component should describe.
fn check_rows(problems: &mut Vec<String>, id: &str, rows: &[u16; 2]) {
    if rows[0] >= rows[1] {
        problems.push(format!(
            "component {id:?} covers the empty row range {rows:?}"
        ));
    }
}

/// The screen text a page describes, one entry per row.
///
/// This is the inverse of describing a screen: a page is not a copy of the grid, so
/// the fact that it can be turned back into the same characters is the property that
/// says nothing was lost on the way. It is checked for every page the corpus produces
/// (see `audit_pages`), which makes it a test over real screens rather than a claim.
///
/// Blank rows carry an empty string. A row inside a component's span that component
/// does not name stays empty, which is what a blank row is.
pub fn render_text(page: &Envelope) -> Vec<String> {
    let mut placed: Vec<(u16, &str)> = Vec::new();
    for component in &page.create_surface.components {
        match &component.body {
            Body::Block {
                rows: span, lines, ..
            } => {
                for (offset, line) in lines.iter().enumerate() {
                    let row = span[0].saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
                    placed.push((row, line.as_str()));
                }
            }
            Body::Diff { rows: span, lines } => {
                for (offset, line) in lines.iter().enumerate() {
                    let row = span[0].saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
                    placed.push((row, line.text.as_str()));
                }
            }
            Body::ToolCall {
                rows: span, calls, ..
            } => {
                for (offset, call) in calls.iter().enumerate() {
                    let row = span[0].saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
                    placed.push((row, call.text.as_str()));
                }
            }
            Body::Meter {
                rows: span, text, ..
            } => placed.push((span[0], text.as_str())),
            Body::Thought {
                rows: span, text, ..
            } => placed.push((span[0], text.as_str())),
            Body::Column { .. } => {}
        }
    }
    let height = placed
        .iter()
        .map(|(row, _)| *row)
        .max()
        .map_or(0, |last| usize::from(last) + 1);
    let mut screen = vec![String::new(); height];
    for (row, text) in placed {
        screen[usize::from(row)] = text.to_owned();
    }
    screen
}

/// How well one page describes the same screen as another.
///
/// Structure and text are scored apart on purpose: a predictor that reproduces every
/// character but puts it in the wrong component is a different failure from one that
/// gets the structure and loses a glyph, and a single number would hide which one
/// happened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Score {
    /// Components in the reference, excluding the root container.
    pub components: usize,
    /// Reference components the other page also has, by kind and content.
    pub matched: usize,
    /// Rows of text in the reference.
    pub rows: usize,
    /// Rows whose text is identical in both pages.
    pub rows_matched: usize,
}

impl Score {
    /// The share of reference components the other page reproduced.
    pub fn structure(&self) -> f64 {
        ratio(self.matched, self.components)
    }

    /// The share of rows whose text is identical.
    pub fn text(&self) -> f64 {
        ratio(self.rows_matched, self.rows)
    }

    /// A page that reproduced everything.
    pub fn is_perfect(&self) -> bool {
        self.matched == self.components && self.rows_matched == self.rows
    }
}

/// A share, with an empty reference counted as fully matched.
///
/// Nothing to reproduce is not a failure to reproduce it: a blank screen scored against
/// a blank screen is a perfect answer, and treating 0/0 as zero would punish a
/// predictor for being right.
fn ratio(matched: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        matched as f64 / total as f64
    }
}

/// Scores `page` against `reference`, which describes the same screen.
///
/// A component matches when the other page has one of the same kind whose text is the
/// same — ids are derived from text, so this is the same test stated without depending
/// on ids matching exactly. Every reference component can be matched at most once, so
/// repeating a component does not earn credit twice.
pub fn compare(reference: &Envelope, page: &Envelope) -> Score {
    let reference_text = render_text(reference);
    let page_text = render_text(page);
    let mut score = Score {
        components: reference
            .create_surface
            .components
            .iter()
            .filter(|component| component.component != "Column")
            .count(),
        rows: reference_text.len(),
        ..Score::default()
    };

    let mut used = vec![false; page.create_surface.components.len()];
    for wanted in &reference.create_surface.components {
        if wanted.component == "Column" {
            continue;
        }
        let wanted_text = component_text(&wanted.body);
        for (index, candidate) in page.create_surface.components.iter().enumerate() {
            if used[index] || candidate.component != wanted.component {
                continue;
            }
            if component_text(&candidate.body) == wanted_text {
                used[index] = true;
                score.matched += 1;
                break;
            }
        }
    }

    for (row, wanted) in reference_text.iter().enumerate() {
        if page_text.get(row) == Some(wanted) {
            score.rows_matched += 1;
        }
    }
    score
}

/// The text a component carries, whatever kind it is.
fn component_text(body: &Body) -> Vec<String> {
    match body {
        Body::Column { .. } => Vec::new(),
        Body::Block { lines, .. } => lines.clone(),
        Body::Diff { lines, .. } => lines.iter().map(|line| line.text.clone()).collect(),
        Body::ToolCall { calls, .. } => calls
            .iter()
            .map(|call| call.text.clone())
            .collect::<Vec<_>>(),
        Body::Meter { text, .. } => vec![text.clone()],
        Body::Thought { text, .. } => vec![text.clone()],
    }
}

/// Where a component's content came from.
///
/// Only `Seen` is produced today. The other variants exist so that a slice which
/// reconstructs or carries content has the word to say so, rather than quietly
/// presenting a guess as an observation (D5, D15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Every character came from the recorded cells.
    Seen,
    /// On screen now, unchanged from an earlier frame.
    Carried,
    /// Reconstructed from context rather than read.
    Guessed,
}

/// An A2UI message carrying one surface.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// The A2UI version this envelope is written to.
    pub version: String,
    /// The surface itself.
    pub create_surface: Surface,
}

/// One surface: its root, its catalogue, and every component in it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Surface {
    /// Identifies this surface within a conversation.
    pub surface_id: String,
    /// The vocabulary the components use.
    pub catalog_id: String,
    /// The id of the component to render first.
    pub root: String,
    /// Every component, as an adjacency list reachable from `root`.
    pub components: Vec<Component>,
}

/// One component in the `moreweb.terminal/v1` catalogue.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Component {
    /// Stable across scrolling: derived from the component's own content.
    pub id: String,
    /// Which catalogue component this is.
    pub component: String,
    /// Where the content came from.
    pub source: Provenance,
    /// The rest of the component, whose shape the catalogue component defines.
    #[serde(flatten)]
    pub body: Body,
}

/// The parts of a component that vary by kind.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Body {
    /// A container: its children, by id.
    Column {
        /// Ids of the components inside this one.
        children: Vec<String>,
    },
    /// A run of rows that read as one thing.
    Block {
        /// What kind of thing the block appears to be.
        role: String,
        /// The rows it covers, as a half-open `[first, end)`.
        rows: [u16; 2],
        /// Its text, one entry per row, right-trimmed.
        lines: Vec<String>,
        /// The styling of each line, parallel to `lines`.
        ///
        /// Only cells drawn differently from the default appear here, so a plain
        /// line contributes an empty list. The plan calls per-span attributes the
        /// largest gap in every surveyed standard, and they are what a `Diff` or an
        /// inline emphasis will be built from.
        spans: Vec<Vec<InlineSpan>>,
        /// The names along a panel's tab strip, when the block is one.
        ///
        /// Read off a recorded overlay: `Context usage  Usage limit  Session info`
        /// above a full-width rule, which is a tab strip rather than a table's column
        /// headers — the words name views, and nothing lines up beneath them. It
        /// belongs to the `Block` because the spec calls a block a generic container
        /// with a role, and inventing a seventh catalogue component for it would
        /// version the vocabulary on the strength of one screen.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tabs: Vec<String>,
        /// Label-and-key rows the block holds, when it draws a shortcut list.
        ///
        /// Read off the application's startup card, where three rows share a right
        /// edge: `Resume session … ctrl+r`, `New worktree … ctrl+w`, `Quit … ctrl+q`.
        /// A runtime can render those as real command buttons, and a model reading
        /// them learns a binding rather than a line of text. Kept on the `Block`
        /// because the card also holds prose, so the block is not itself a table.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        shortcuts: Vec<Shortcut>,
        /// Named values the block reports, when it draws an information panel.
        ///
        /// Read off a recorded `/session-info` overlay: `Shell version:` with `1.0.70`
        /// beneath it, and `Model: Grok 4.7` on one line. A runtime renders those as
        /// fields rather than as prose, and a model reading the page learns that
        /// `Model` has the value `Grok 4.7` instead of learning a line of text.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        properties: Vec<Property>,
    },
    /// A block that reads as an edit to a file.
    ///
    /// This is the component ADR 0002 calls the thing no surveyed standard can
    /// express: a diff needs a per-line kind *and* per-span attributes on the same
    /// line, and no standard has both. It is recognised from the text alone — a
    /// `+` line and a `-` line in the same block — because the prefixes are what the
    /// application draws regardless of theme, while the colours it uses to reinforce
    /// them are not something to depend on.
    Diff {
        /// The rows it covers, as a half-open `[first, end)`.
        rows: [u16; 2],
        /// The lines of the edit, in order.
        lines: Vec<DiffLine>,
    },
    /// A run of tool lines: what the application read, ran or edited.
    ToolCall {
        /// The rows it covers, as a half-open `[first, end)`.
        rows: [u16; 2],
        /// One entry per line, in the order drawn.
        calls: Vec<ToolCallLine>,
    },
    /// A block of reasoning the application collapsed to a summary line.
    ///
    /// Read off a recorded screen with reasoning raised: `♦ Thought for 3.0s`. The
    /// reasoning itself is not on the page — collapsed, not merely styled — so it is not
    /// in the page either. That is the spec's folding rule applied honestly: a folded
    /// block's hidden content is not in the IR, and a page that invented it would be
    /// describing a screen nobody saw.
    Thought {
        /// The rows it covers, as a half-open `[first, end)`.
        rows: [u16; 2],
        /// The summary line as drawn.
        text: String,
        /// How long the reasoning took, in seconds, when the line reports it.
        #[serde(skip_serializing_if = "Option::is_none")]
        seconds: Option<f64>,
    },
    /// A line reporting a quantity against a limit: usage, a quota, a budget.
    ///
    /// Read off a real frame: `1.3K / 500K │ [Dashboard]`. The drawn strings are kept
    /// verbatim with the fraction computed beside them, so a renderer that wants a bar
    /// and a reader that wants the number do not have to parse the screen for either.
    Meter {
        /// The rows it covers, as a half-open `[first, end)`.
        rows: [u16; 2],
        /// How much has been used, as drawn.
        used: String,
        /// What it is measured against, as drawn.
        limit: String,
        /// `used / limit`, when both sides are numbers.
        #[serde(skip_serializing_if = "Option::is_none")]
        fraction: Option<f64>,
        /// What the meter is about, when the line names it.
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// The whole line as drawn.
        text: String,
    },
}

/// One line of a diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    /// `add`, `remove` or `context`.
    pub kind: String,
    /// The line's text, right-trimmed, including its `+` or `-` prefix.
    pub text: String,
    /// The styling of the line, as a block's lines carry it.
    pub spans: Vec<InlineSpan>,
}

/// The glyph the application puts in front of a tool line.
///
/// Read off recorded screens: `♦ Read notes.txt`, `♦ Edited notes.txt, explored 1
/// file +1 -1`. A glyph is a rendering choice and this one could change, so the
/// only thing it is used for is *splitting* a transcript — anything it does not
/// match stays an ordinary block, which is a smaller mistake than mislabelling prose.
const TOOL_GLYPH: char = '♦';

/// A run of tool lines inside a transcript block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ToolCallLine {
    /// Lifecycle state. `done` for every line this slice can see, because the glyph
    /// marks a call the application has finished drawing.
    pub status: String,
    /// The tool the application named, e.g. `Read` or `Edited`.
    pub tool: String,
    /// What it was applied to, when the line names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Lines added, for an edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    /// Lines removed, for an edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
    /// The line as drawn, for a renderer that wants to show it verbatim.
    pub text: String,
}

/// A run of cells inside one line that shares a non-default style.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InlineSpan {
    /// First column of the run, counting from the line's left edge.
    pub start: u16,
    /// One past the last column of the run.
    pub end: u16,
    /// How those cells were drawn.
    pub style: Emphasis,
}

/// The attributes of a styled run, in terms a renderer can act on.
///
/// Colour is a string rather than an enum because the catalogue's consumers are
/// renderers of several languages, and `indexed:4` / `rgb:12,34,56` needs no shared
/// type to be read. It is omitted when the terminal's default was in force, which is
/// the common case and the one worth keeping out of the file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Emphasis {
    /// Bold or increased intensity.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Foreground and background swapped.
    pub reverse: bool,
    /// Foreground colour, when it was not the terminal's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    /// Background colour, when it was not the terminal's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
}

/// The id of the surface's root container.
const ROOT_ID: &str = "root";

/// Builds the page a frame describes, with every block marked `seen`.
///
/// `surface_id` names the surface within a conversation; a recording usually uses
/// its session name, so two captures of the same application stay distinguishable.
///
/// This is the single-frame view, where nothing can be `carried` yet because there
/// is no earlier frame to carry from. Use [`pages`] to describe a whole session and
/// get the provenance the vocabulary exists for.
pub fn page_from_frame(frame: &Frame, surface_id: &str) -> Envelope {
    let mut appeared = HashSet::new();
    page_with(frame, surface_id, &mut appeared)
}

/// Builds one page per frame, in order, carrying provenance across them.
///
/// A block whose text has appeared in an earlier frame of the same session is marked
/// `carried`: it is on screen now, unchanged, not newly drawn. Everything else is
/// `seen`, because every character of it was read from the cells.
///
/// The distinction is the point of describing a sequence rather than a screen. A
/// consumer replaying these pages can tell what actually changed between two
/// moments without diffing the JSON, and the plan's provenance rule (D5) is
/// satisfied by observation rather than by assertion.
pub fn pages(frames: &[Frame], surface_id: &str) -> Vec<Envelope> {
    let mut appeared = HashSet::new();
    frames
        .iter()
        .map(|frame| page_with(frame, surface_id, &mut appeared))
        .collect()
}

/// Builds one page, recording which block ids have appeared so far.
fn page_with(frame: &Frame, surface_id: &str, appeared: &mut HashSet<String>) -> Envelope {
    let grid = decode_grid(&frame.runs, frame.rows, frame.cols);
    let lines: Vec<String> = grid
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.ch)
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    // Styling is read over the same width the text was trimmed to, so a span can
    // never point past the end of the line it belongs to.
    let spans: Vec<Vec<InlineSpan>> = grid
        .iter()
        .zip(&lines)
        .map(|(row, line)| spans_of(row, line.chars().count()))
        .collect();

    let mut components = vec![Component {
        id: ROOT_ID.to_owned(),
        component: "Column".to_owned(),
        source: Provenance::Seen,
        body: Body::Column {
            children: Vec::new(),
        },
    }];
    let mut children = Vec::new();
    for (first, last) in blocks(&lines) {
        // A transcript block mixes prose with tool lines, so the block is split at
        // the tool lines: a run of tool lines is one `ToolCall`, and a run of
        // anything else is one `Block`. Keeping them together would leave the tool
        // calls buried inside a paragraph, which is the opposite of the structure a
        // page is for.
        for segment in segments(&lines, first, last) {
            let text = &lines[segment.first..segment.last];
            let segment_spans = spans[segment.first..segment.last].to_vec();
            let rows = [
                u16::try_from(segment.first).unwrap_or(u16::MAX),
                u16::try_from(segment.last).unwrap_or(u16::MAX),
            ];
            let id = block_id(text);
            children.push(id.clone());
            // `insert` is the whole provenance rule: the first frame to show this
            // content read it from the cells, every later one carried it forward.
            let source = if appeared.insert(id.clone()) {
                Provenance::Seen
            } else {
                Provenance::Carried
            };
            let (component, body) = if segment.tool {
                let calls = tool_calls(text);
                // A collapsed reasoning line is the catalogue's `Thought`, not a tool
                // call: the application says `♦ Thought for 3.0s` and nothing else.
                match calls.as_slice() {
                    [only] if only.tool.eq_ignore_ascii_case("thought") => (
                        "Thought".to_owned(),
                        Body::Thought {
                            rows,
                            text: only.text.trim_end().to_owned(),
                            seconds: thought_seconds(&only.text),
                        },
                    ),
                    _ => ("ToolCall".to_owned(), Body::ToolCall { rows, calls }),
                }
            } else if let Some(meter) = as_meter(text) {
                (
                    "Meter".to_owned(),
                    Body::Meter {
                        rows,
                        used: meter.used,
                        limit: meter.limit,
                        fraction: meter.fraction,
                        label: meter.label,
                        text: meter.text,
                    },
                )
            } else if let Some(diff) = as_diff(text, &segment_spans) {
                ("Diff".to_owned(), Body::Diff { rows, lines: diff })
            } else {
                (
                    "Block".to_owned(),
                    Body::Block {
                        role: role_of(text).to_owned(),
                        rows,
                        lines: text.to_vec(),
                        spans: segment_spans,
                        tabs: tabs_of(text),
                        shortcuts: shortcuts_of(text),
                        properties: properties_of(text),
                    },
                )
            };
            components.push(Component {
                id,
                component,
                source,
                body,
            });
        }
    }
    // The root's children are only known once every block has been walked.
    if let Some(Body::Column {
        children: root_children,
    }) = components.first_mut().map(|component| &mut component.body)
    {
        *root_children = children;
    }

    Envelope {
        version: ENVELOPE_VERSION.to_owned(),
        create_surface: Surface {
            surface_id: surface_id.to_owned(),
            catalog_id: CATALOGUE_ID.to_owned(),
            root: ROOT_ID.to_owned(),
            components,
        },
    }
}

/// The lines of a block read as a diff, or `None` when the block is not one.
///
/// A diff is recognised by carrying **both** a `+` line and a `-` line: those are
/// the prefixes the application draws for an addition and a deletion, whatever
/// theme it is running. Requiring the pair is what keeps a bulleted list — which has
/// `-` lines and no `+` lines — from being described as an edit that removes content
/// nobody removed. A diff that only adds lines is therefore reported as a `Block`,
/// which is the conservative direction: a plain block is never wrong, only less
/// specific.
fn as_diff(lines: &[String], spans: &[Vec<InlineSpan>]) -> Option<Vec<DiffLine>> {
    let kinds: Vec<&str> = lines
        .iter()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('+') {
                "add"
            } else if trimmed.starts_with('-') {
                "remove"
            } else {
                "context"
            }
        })
        .collect();
    let adds = kinds.iter().filter(|kind| **kind == "add").count();
    let removes = kinds.iter().filter(|kind| **kind == "remove").count();
    if adds == 0 || removes == 0 {
        return None;
    }
    Some(
        lines
            .iter()
            .zip(spans)
            .zip(&kinds)
            .map(|((text, spans), kind)| DiffLine {
                kind: (*kind).to_owned(),
                text: text.clone(),
                spans: spans.clone(),
            })
            .collect(),
    )
}

/// A stretch of a transcript block that is one thing.
struct Segment {
    /// First row of the stretch.
    first: usize,
    /// One past its last row.
    last: usize,
    /// True when every line in it is a tool line.
    tool: bool,
    /// True when the stretch is a bordered box: a panel, or the composer.
    boxed: bool,
}

/// Splits one block into the things it holds.
///
/// A block is a run of non-blank rows, and a run of non-blank rows can hold several
/// things: prose, tool lines, and a bordered box. A recorded `/help` overlay made this
/// concrete — the panel and the composer beneath it touch, so they arrived as one block
/// of thirty-three lines and the whole thing was described as a composer. Splitting at
/// the borders is what makes the panel a panel and the composer a composer.
fn segments(lines: &[String], first: usize, last: usize) -> Vec<Segment> {
    if first >= last {
        return Vec::new();
    }
    let mut found: Vec<Segment> = Vec::new();
    let mut inside_box = false;
    for (row, line) in lines.iter().enumerate().take(last).skip(first) {
        let opens = opens_box(line);
        let closes = closes_box(line);
        let tool = is_tool_line(line);

        // A box that opens ends whatever stretch was running, and the box takes its own
        // first line; a box that closes ends its own stretch after the line.
        // Any newly opening box starts its own stretch, even when one is already open:
        // an overlay drawn over a panel can have its bottom border scrolled off, and the
        // composer below it is a different thing regardless. A recorded `/help` screen
        // is exactly that case.
        if opens {
            inside_box = true;
            found.push(Segment {
                first: row,
                last: row + 1,
                tool: false,
                boxed: true,
            });
            continue;
        }
        if inside_box {
            if let Some(current) = found.last_mut() {
                current.last = row + 1;
            }
            if closes {
                inside_box = false;
            }
            continue;
        }
        if closes && !inside_box {
            // A closing border with no opening: its own stretch, so the shape is kept.
            found.push(Segment {
                first: row,
                last: row + 1,
                tool: false,
                boxed: true,
            });
            continue;
        }
        // Outside any box: extend the last stretch when it is the same kind, else start
        // a new one.
        match found.last_mut() {
            Some(current) if !current.boxed && current.tool == tool => current.last = row + 1,
            _ => found.push(Segment {
                first: row,
                last: row + 1,
                tool,
                boxed: false,
            }),
        }
    }
    found
}

/// Whether a line opens a bordered box.
fn opens_box(line: &str) -> bool {
    line.trim_start().starts_with(['╭', '┌'])
}

/// Whether a line closes a bordered box.
fn closes_box(line: &str) -> bool {
    line.trim_start().starts_with(['╰', '└'])
}

/// Whether a line is a tool line.
fn is_tool_line(line: &str) -> bool {
    line.trim_start().starts_with(TOOL_GLYPH)
}

/// The tool calls a run of tool lines describes.
fn tool_calls(lines: &[String]) -> Vec<ToolCallLine> {
    lines.iter().map(|line| parse_tool_line(line)).collect()
}

/// Reads one tool line: `♦ Edited notes.txt, explored 1 file +1 -1`.
///
/// The shape is taken from recorded screens rather than from documentation, so it is
/// read leniently: a line with no tool word, or no counts, still becomes a call with
/// whatever was there.
///
/// `text` keeps the line exactly as drawn, leading indent included: rendering a page
/// back to a screen has to put the glyph where the application put it, and trimming
/// here would silently move it to the margin.
fn parse_tool_line(line: &str) -> ToolCallLine {
    let body = line
        .trim_start()
        .strip_prefix(TOOL_GLYPH)
        .unwrap_or(line.trim_start())
        .trim_start();
    let (tool, rest) = match body.find(' ') {
        Some(space) => (&body[..space], body[space + 1..].trim_start()),
        None => (body, ""),
    };
    let target = target_of(rest);
    ToolCallLine {
        status: "done".to_owned(),
        tool: tool.to_owned(),
        target: (!target.is_empty()).then(|| target.to_owned()),
        added: count_after(rest, '+'),
        removed: count_after(rest, '-'),
        text: line.to_owned(),
    }
}

/// The target a tool line names: `notes.txt` out of `notes.txt, explored 1 file`.
///
/// A terminal row runs to the edge of the screen, so the rest of a tool line holds
/// padding and whatever the application draws in its gutter — a scrollbar, a
/// timestamp. Two or more spaces separate one column from another and a comma
/// separates the target from the sentence after it, so the target ends at whichever
/// comes first. Without this a real frame yields `notes.txt` followed by forty spaces
/// and the scrollbar glyph, which was the first thing a captured screen proved.
fn target_of(rest: &str) -> &str {
    let mut end = rest.len();
    let mut spaces = 0;
    for (index, character) in rest.char_indices() {
        if character == ',' {
            end = end.min(index);
            break;
        }
        if character == ' ' {
            spaces += 1;
            if spaces == 2 {
                end = end.min(index.saturating_sub(1));
                break;
            }
        } else {
            spaces = 0;
        }
    }
    rest[..end].trim()
}

/// The number in `<sign><digits>`, when the text holds one.
///
/// The sign must not follow a word character, so `explored 1 file` cannot contribute
/// a negative count and `a-b` is not read as `-b`.
fn count_after(text: &str, sign: char) -> Option<u32> {
    let characters: Vec<char> = text.chars().collect();
    for index in 0..characters.len().saturating_sub(1) {
        if characters[index] != sign || !characters[index + 1].is_ascii_digit() {
            continue;
        }
        if index > 0 && characters[index - 1].is_alphanumeric() {
            continue;
        }
        let digits: String = characters[index + 1..]
            .iter()
            .take_while(|character| character.is_ascii_digit())
            .collect();
        if let Ok(count) = digits.parse() {
            return Some(count);
        }
    }
    None
}

/// The parts of a meter, before it is placed on rows.
struct MeterParts {
    /// How much has been used, as drawn.
    used: String,
    /// What it is measured against, as drawn.
    limit: String,
    /// `used / limit`, when both sides are numbers.
    fraction: Option<f64>,
    /// What the meter is about, when the line names it.
    label: Option<String>,
    /// The whole line as drawn.
    text: String,
}

/// Reads a line as a quantity against a limit, or `None` when it is not one.
///
/// The shape is `<number> / <number>` among the line's whitespace-separated words,
/// which is what a real frame draws: `1.3K / 500K │ [Dashboard]`. Requiring both
/// neighbours of the `/` to be numbers is what keeps prose and paths out — a path
/// containing a slash has a word, not a number, on each side of it.
fn as_meter(lines: &[String]) -> Option<MeterParts> {
    let line = lines.first()?;
    let words: Vec<&str> = line.split_whitespace().collect();
    let slash = words.iter().position(|word| *word == "/")?;
    let used = *words.get(slash.checked_sub(1)?)?;
    let limit = *words.get(slash + 1)?;
    let used_value = number(used)?;
    let limit_value = number(limit)?;
    let label = words
        .iter()
        // Both separators, because which one an application draws depends on the
        // terminal it thinks it is talking to, and a label is worth reading either way.
        .position(|word| *word == "│" || *word == "|")
        .map(|bar| words[bar + 1..].join(" "))
        .filter(|label| !label.is_empty());
    Some(MeterParts {
        used: used.to_owned(),
        limit: limit.to_owned(),
        fraction: (limit_value > 0.0).then(|| used_value / limit_value),
        label,
        text: line.clone(),
    })
}

/// A number as drawn, with the multiplier a suffix stands for.
fn number(word: &str) -> Option<f64> {
    let (digits, multiplier) = match word.chars().last()? {
        'K' => (&word[..word.len() - 1], 1e3),
        'M' => (&word[..word.len() - 1], 1e6),
        'G' => (&word[..word.len() - 1], 1e9),
        _ => (word, 1.0),
    };
    let value: f64 = digits.parse().ok()?;
    Some(value * multiplier)
}

/// The runs of a line that were drawn with a non-default style.
///
/// `width` is how many characters of the row the line kept, so a run is reported
/// against the trimmed text a consumer actually reads.
fn spans_of(row: &[StyledCell], width: usize) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut open: Option<(u16, Emphasis)> = None;
    for (col, cell) in row.iter().take(width.min(row.len())).enumerate() {
        let column = u16::try_from(col).unwrap_or(u16::MAX);
        let emphasis = emphasis_of(&cell.style);
        match (open.take(), emphasis) {
            (Some((start, current)), Some(next)) if current == next => {
                open = Some((start, current));
            }
            (Some((start, current)), next) => {
                spans.push(InlineSpan {
                    start,
                    end: column,
                    style: current,
                });
                open = next.map(|next| (column, next));
            }
            (None, Some(next)) => open = Some((column, next)),
            (None, None) => {}
        }
    }
    if let Some((start, style)) = open {
        spans.push(InlineSpan {
            start,
            end: u16::try_from(width).unwrap_or(u16::MAX),
            style,
        });
    }
    spans
}

/// How a cell was drawn, or `None` when it was drawn plainly.
fn emphasis_of(style: &Style) -> Option<Emphasis> {
    let colour = |colour: &Color| match colour {
        Color::Default => None,
        Color::Indexed(index) => Some(format!("indexed:{index}")),
        Color::Rgb(r, g, b) => Some(format!("rgb:{r},{g},{b}")),
    };
    let emphasis = Emphasis {
        bold: style.bold,
        italic: style.italic,
        underline: style.underline,
        reverse: style.reverse,
        fg: colour(&style.fg),
        bg: colour(&style.bg),
    };
    (emphasis != Emphasis::default()).then_some(emphasis)
}

/// The half-open row ranges of the blocks: runs of rows that are not blank.
fn blocks(lines: &[String]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut start = None;
    for (row, line) in lines.iter().enumerate() {
        let blank = line.trim().is_empty();
        match (blank, start) {
            (false, None) => start = Some(row),
            (true, Some(first)) => {
                found.push((first, row));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        found.push((first, lines.len()));
    }
    found
}

/// How long a collapsed reasoning line reports it took, in seconds.
///
/// The line is `♦ Thought for 3.0s`, so the number sits between `for` and `s`. A line
/// without one is still a thought; it simply does not claim a duration.
fn thought_seconds(text: &str) -> Option<f64> {
    let after_for = text.split_once(" for ")?.1;
    let digits: String = after_for
        .trim_start()
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    digits.parse().ok()
}

/// One row of a shortcut list: what it does, and the key that does it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Shortcut {
    /// The action, as drawn: `Resume session`.
    pub label: String,
    /// The binding, as drawn: `ctrl+r`.
    pub key: String,
}

/// One named value: a property an information panel reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Property {
    /// The name, as drawn: `Model`.
    pub label: String,
    /// The value, as drawn: `Grok 4.7`.
    pub value: String,
}

/// The named values a block reports.
///
/// Two shapes are read, because the same panel uses both: `Model: Grok 4.7` on one line,
/// and a label line `Shell version:` with its value on the line beneath. A label with
/// nothing after it and nothing beneath is not a property — it is a heading or a stub.
fn properties_of(lines: &[String]) -> Vec<Property> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = strip_borders(&lines[index]);
        let trimmed = line.trim();
        if let Some((label, value)) = split_property(trimmed) {
            if !value.is_empty() {
                found.push(Property { label, value });
                index += 1;
                continue;
            }
            // A label on its own line takes the next row as its value, if that row
            // reads as a value rather than as another label.
            if let Some(next) = lines.get(index + 1).map(|next| strip_borders(next)) {
                let next = next.trim();
                if !next.is_empty() && split_property(next).is_none() {
                    found.push(Property {
                        label,
                        value: next.to_owned(),
                    });
                    index += 2;
                    continue;
                }
            }
        }
        index += 1;
    }
    found
}

/// A line split into a label and a value at its first colon, when it has one.
fn split_property(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let label = line[..colon].trim();
    // One character is not a name: `C:\Users\…` is a Windows path, and reading its
    // drive letter as a property called `C` was the first thing a real overlay did.
    if label.chars().count() < 2 || label.chars().count() > 32 || label.contains(':') {
        return None;
    }
    // A label is words; a sentence with a colon in it is prose.
    if label.split_whitespace().count() > 4 {
        return None;
    }
    Some((label.to_owned(), line[colon + 1..].trim().to_owned()))
}

/// The shortcut rows a block holds.
///
/// A row is two fields separated by a wide gap where the right field ends at the same
/// column as at least one other row — that shared edge is what a shortcut list has and
/// a paragraph does not. Rows that merely contain a wide gap are left alone.
fn shortcuts_of(lines: &[String]) -> Vec<Shortcut> {
    let candidates: Vec<(usize, String, String)> = lines
        .iter()
        .filter_map(|line| {
            let stripped = strip_borders(line);
            let (gap, width) = wide_gap(&stripped)?;
            let label = stripped[..gap].trim();
            let key = stripped[gap + width..].trim();
            if label.is_empty() || key.is_empty() || key.chars().count() > 24 {
                return None;
            }
            Some((
                gap + width + key.chars().count(),
                label.to_owned(),
                key.to_owned(),
            ))
        })
        .collect();

    // The right edge that at least two rows share.
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (edge, _, _) in &candidates {
        match edges.iter_mut().find(|(seen, _)| seen == edge) {
            Some((_, count)) => *count += 1,
            None => edges.push((*edge, 1)),
        }
    }
    let shared = edges
        .iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(edge, _)| *edge)
        .max();
    let Some(edge) = shared else {
        return Vec::new();
    };
    candidates
        .into_iter()
        .filter(|(row_edge, _, _)| *row_edge == edge)
        .map(|(_, label, key)| Shortcut { label, key })
        .collect()
}

/// A line without the panel borders an application draws around content.
///
/// A scrollbar is stripped too: it is drawn in the panel's gutter, not written by the
/// content, and leaving it on the last field would make every row of a list end at a
/// different place than the one beside it.
fn strip_borders(line: &str) -> String {
    let trimmed = line.trim();
    let trimmed = trimmed
        .strip_prefix(['│', '┃'])
        .unwrap_or(trimmed)
        .trim_start();
    trimmed
        .trim_end_matches(['█', '▌', '▐', '│', '┃'])
        .trim_end()
        .to_owned()
}

/// Where a line's first wide gap starts, and how wide it is.
fn wide_gap(line: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut width = 0;
    for (index, character) in line.char_indices() {
        if character == ' ' {
            if start.is_none() {
                start = Some(index);
            }
            width += 1;
            continue;
        }
        if let Some(begin) = start {
            if width >= 2 && begin > 0 {
                return Some((begin, width));
            }
        }
        start = None;
        width = 0;
    }
    None
}

/// The tab strip a panel's block carries, or nothing when it has none.
///
/// A tab strip is a line of two or more names separated by wide gaps, sitting directly
/// above a rule that spans the panel. Requiring the rule is what separates it from two
/// columns of content: the rule is the thing that says "these are views, and what is
/// below is what the selected one shows".
fn tabs_of(lines: &[String]) -> Vec<String> {
    let Some(rule) = lines.iter().position(|line| is_rule(line)) else {
        return Vec::new();
    };
    let Some(names) = rule.checked_sub(1).map(|above| &lines[above]) else {
        return Vec::new();
    };
    let fields = fields_of(names);
    if fields.len() < 2 {
        return Vec::new();
    }
    fields
}

/// Whether a line is a horizontal rule drawn across a panel.
///
/// Only the box-drawing characters a terminal uses for one, after the surrounding
/// borders are removed: a line of dashes in prose is not a rule.
fn is_rule(line: &str) -> bool {
    let trimmed = line
        .trim()
        .trim_matches(|character| character == '│' || character == '┃');
    let trimmed = trimmed.trim();
    !trimmed.is_empty() && trimmed.chars().all(|character| "─━═".contains(character))
}

/// A line split into fields at gaps of two or more spaces.
///
/// Single spaces belong to a field (`Context usage` is one name), which is why this is
/// not `split_whitespace`: that would turn one tab into two.
fn fields_of(line: &str) -> Vec<String> {
    let trimmed = line
        .trim()
        .trim_matches(|character| character == '│' || character == '┃');
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut spaces = 0;
    for character in trimmed.chars() {
        if character == ' ' {
            spaces += 1;
            continue;
        }
        if spaces >= 2 && !current.trim().is_empty() {
            fields.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push_str(&" ".repeat(spaces));
        }
        spaces = 0;
        current.push(character);
    }
    if !current.trim().is_empty() {
        fields.push(current.trim().to_owned());
    }
    fields
}

/// A role, from what the block's own text says about itself.
///
/// Only roles the text can prove are named; everything else is `text` rather than a
/// confident guess that would be wrong on the page.
fn role_of(lines: &[String]) -> &'static str {
    if is_composer(lines) {
        return "composer";
    }
    if is_panel(lines) {
        return "panel";
    }
    let first = lines.first().map(String::as_str).unwrap_or_default();
    let trimmed = first.trim_start();
    if trimmed.starts_with('>') {
        "prompt"
    } else if trimmed.starts_with('#') {
        "heading"
    } else {
        "text"
    }
}

/// Whether a block is a bordered container rather than a stretch of text.
///
/// A panel opens with a corner rule and closes with one, which is how an application
/// draws an overlay or a card. It is a role rather than a component because the spec's
/// `Block` is "a generic container with a role", and because a panel and a paragraph
/// differ in what a renderer would draw, not in what the cells say.
fn is_panel(lines: &[String]) -> bool {
    let opens = lines
        .first()
        .is_some_and(|line| line.trim_start().starts_with(['╭', '┌']));
    let closes = lines
        .last()
        .is_some_and(|line| line.trim_start().starts_with(['╰', '└']));
    opens && closes && lines.len() >= 3
}

/// Whether a block is the application's input box.
///
/// Read off recorded screens, where it is the same three lines on every one of them:
/// a rounded top border, a row holding the text cursor prompt, and a rounded bottom
/// border. It is worth naming because it is the surface a GUI would have to give back
/// as an editable field, and a generic block does not say so.
///
/// The test is structural rather than positional: the borders must be the first and
/// last lines of the block and the prompt must be inside them, so a panel that happens
/// to be rounded does not become an input box just by being drawn in a box.
fn is_composer(lines: &[String]) -> bool {
    let Some(first) = lines.first() else {
        return false;
    };
    let Some(last) = lines.last() else {
        return false;
    };
    // Rounded and square corners both, because which an application draws depends on
    // the glyph set it believes the terminal has: the one that draws `╭` here draws
    // `┌` when it is talking to a code page without the rounded characters.
    let opens = first.trim_start().starts_with(['╭', '┌']);
    let closes = last.trim_start().starts_with(['╰', '└']);
    // A block of a single line cannot hold a border on both sides of itself.
    let prompt_between = lines.len() >= 2
        && lines[1..lines.len() - 1]
            .iter()
            .any(|line| line.trim_start().starts_with('│') && line.contains('>'));
    opens && closes && prompt_between
}

/// A stable id for a block, derived from its content.
///
/// FNV-1a over the text: the same block scrolled to another row keeps its id, which
/// a positional id could not do.
fn block_id(lines: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for line in lines {
        for byte in line.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // A separator, so ["ab", "c"] and ["a", "bc"] are not the same block.
        hash ^= 0x0a;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("block-{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_lm_frames::{frame_from_chars, CellRun};

    /// A frame whose rows are the given lines, padded to `cols`.
    fn frame_of(rows: &[&str], cols: u16) -> Frame {
        let chars: Vec<Vec<char>> = rows.iter().map(|line| line.chars().collect()).collect();
        frame_from_chars(0, u16::try_from(rows.len()).unwrap(), cols, &chars)
    }

    #[test]
    fn blocks_are_runs_of_unblank_rows() {
        let frame = frame_of(&["one", "", "two", "three", "", "", "four"], 10);
        let page = page_from_frame(&frame, "s");
        let blocks: Vec<_> = page
            .create_surface
            .components
            .iter()
            .filter(|component| component.component == "Block")
            .collect();
        assert_eq!(blocks.len(), 3, "{:?}", page);
        assert_eq!(
            blocks[0].body,
            Body::Block {
                role: "text".to_owned(),
                rows: [0, 1],
                lines: vec!["one".to_owned()],
                tabs: Vec::new(),
                shortcuts: Vec::new(),
                properties: Vec::new(),
                spans: vec![Vec::new()],
            }
        );
        assert_eq!(
            blocks[1].body,
            Body::Block {
                role: "text".to_owned(),
                rows: [2, 4],
                lines: vec!["two".to_owned(), "three".to_owned()],
                tabs: Vec::new(),
                shortcuts: Vec::new(),
                properties: Vec::new(),
                spans: vec![Vec::new(), Vec::new()],
            }
        );
    }

    #[test]
    fn a_styled_run_becomes_a_span_and_a_plain_one_does_not() {
        // Styling is the thing the grid had and the page was throwing away. A cell
        // drawn boldly must come back as a run a renderer can act on; a plainly
        // drawn line must contribute nothing at all.
        let mut frame = Frame::empty(1, 4);
        frame.runs = vec![
            CellRun {
                row: 0,
                col: 0,
                len: 2,
                ch: 'h',
                style: Style {
                    bold: true,
                    ..Style::default()
                },
                wide_tail: false,
            },
            CellRun {
                row: 0,
                col: 2,
                len: 2,
                ch: 'o',
                style: Style::default(),
                wide_tail: false,
            },
        ];
        let page = page_from_frame(&frame, "s");
        let Some(Body::Block { spans, lines, .. }) = page
            .create_surface
            .components
            .iter()
            .find(|component| component.component == "Block")
            .map(|component| &component.body)
        else {
            panic!("the styled row must be a block: {:?}", page);
        };
        assert_eq!(lines, &vec!["hhoo".to_owned()]);
        assert_eq!(
            spans,
            &vec![vec![InlineSpan {
                start: 0,
                end: 2,
                style: Emphasis {
                    bold: true,
                    ..Emphasis::default()
                },
            }]]
        );
    }

    #[test]
    fn the_envelope_names_the_catalogue_and_the_root() {
        let page = page_from_frame(&frame_of(&["hello"], 10), "w1-s01");
        assert_eq!(page.version, "v0.9.1");
        assert_eq!(page.create_surface.catalog_id, "moreweb.terminal/v1");
        assert_eq!(page.create_surface.surface_id, "w1-s01");
        assert_eq!(page.create_surface.root, "root");
        let root = &page.create_surface.components[0];
        let Body::Column { children } = &root.body else {
            panic!("the first component must be the root container: {root:?}");
        };
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn a_block_keeps_its_id_when_it_scrolls() {
        // The same block one row lower is the same block: positional ids would give
        // these two different ids and break anything keyed on them.
        let upper = page_from_frame(&frame_of(&["first", "second"], 10), "s");
        let lower = page_from_frame(&frame_of(&["", "first", "second"], 10), "s");
        let id_of = |page: &Envelope| {
            page.create_surface
                .components
                .iter()
                .find(|component| {
                    matches!(&component.body, Body::Block { lines, .. } if lines[0] == "first")
                })
                .map(|component| component.id.clone())
                .expect("the block is in both pages")
        };
        assert_eq!(id_of(&upper), id_of(&lower));
    }

    #[test]
    fn two_different_blocks_do_not_share_an_id() {
        let page = page_from_frame(&frame_of(&["alpha", "", "beta"], 10), "s");
        let ids: Vec<&str> = page
            .create_surface
            .components
            .iter()
            .filter(|component| component.component == "Block")
            .map(|component| component.id.as_str())
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn every_component_is_marked_seen_because_every_character_was() {
        let page = page_from_frame(&frame_of(&["a", "", "> prompt"], 12), "s");
        for component in &page.create_surface.components {
            assert_eq!(component.source, Provenance::Seen, "{component:?}");
        }
        let roles: Vec<&str> = page
            .create_surface
            .components
            .iter()
            .filter_map(|component| match &component.body {
                Body::Block { role, .. } => Some(role.as_str()),
                Body::Column { .. }
                | Body::Diff { .. }
                | Body::ToolCall { .. }
                | Body::Meter { .. }
                | Body::Thought { .. } => None,
            })
            .collect();
        assert_eq!(roles, vec!["text", "prompt"]);
    }

    #[test]
    fn a_block_with_an_added_and_a_removed_line_is_a_diff() {
        let page = page_from_frame(
            &frame_of(&["  -old line", "  +new line", "   context"], 20),
            "s",
        );
        let diff = page
            .create_surface
            .components
            .iter()
            .find(|component| component.component == "Diff")
            .unwrap_or_else(|| panic!("the edit must be a Diff: {page:?}"));
        let Body::Diff { lines, rows } = &diff.body else {
            panic!("a Diff component must carry diff lines: {diff:?}");
        };
        assert_eq!(rows, &[0, 3]);
        let kinds: Vec<&str> = lines.iter().map(|line| line.kind.as_str()).collect();
        assert_eq!(kinds, vec!["remove", "add", "context"]);
    }

    #[test]
    fn a_bulleted_list_is_not_a_diff() {
        // The reason the detection needs both prefixes: three list items look like
        // three deletions to a rule that only looks for `-`.
        let page = page_from_frame(&frame_of(&["- one", "- two", "- three"], 12), "s");
        assert!(
            page.create_surface
                .components
                .iter()
                .all(|component| component.component != "Diff"),
            "a list must not be described as an edit: {page:?}"
        );
    }

    #[test]
    fn a_blank_screen_has_a_root_and_no_blocks() {
        let page = page_from_frame(&frame_of(&["", "", ""], 5), "s");
        assert_eq!(page.create_surface.components.len(), 1);
        assert!(matches!(
            &page.create_surface.components[0].body,
            Body::Column { children } if children.is_empty()
        ));
    }

    #[test]
    fn a_block_on_screen_twice_is_carried_the_second_time() {
        // The page sequence is where `carried` earns its place: the same block in
        // two frames is read once and carried after that, while a block that only
        // exists in the later frame was seen there.
        let first = frame_of(&["keep", "", "also"], 12);
        let second = frame_of(&["keep", "", "also", "", "new"], 12);
        let described = pages(&[first, second], "s");
        let source_of = |page: &Envelope, prefix: &str| {
            page.create_surface
                .components
                .iter()
                .find_map(|component| match &component.body {
                    Body::Block { lines, .. } if lines[0].starts_with(prefix) => {
                        Some(component.source)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no block starting {prefix:?} in {page:?}"))
        };
        assert_eq!(source_of(&described[0], "keep"), Provenance::Seen);
        assert_eq!(source_of(&described[1], "keep"), Provenance::Carried);
        assert_eq!(source_of(&described[1], "also"), Provenance::Carried);
        assert_eq!(
            source_of(&described[1], "new"),
            Provenance::Seen,
            "a block that only exists in the later frame cannot have been carried"
        );
    }

    #[test]
    fn a_transcript_splits_at_its_tool_lines() {
        // The shape recorded from a real screen: prose, a tool line, then more prose,
        // all inside one block of non-blank rows. The tool call must come out as its
        // own component, not be buried in the paragraph.
        let page = page_from_frame(
            &frame_of(
                &[
                    "I will update it.",
                    "  ♦ Read notes.txt",
                    "  ♦ Edited notes.txt, explored 1 file +1 -1",
                    "The second line is now BETA.",
                ],
                70,
            ),
            "s",
        );
        let names: Vec<&str> = page
            .create_surface
            .components
            .iter()
            .map(|component| component.component.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["Column", "Block", "ToolCall", "Block"],
            "{page:?}"
        );

        let call = page
            .create_surface
            .components
            .iter()
            .find(|component| component.component == "ToolCall")
            .expect("the tool run must be a ToolCall");
        let Body::ToolCall { calls, rows } = &call.body else {
            panic!("a ToolCall component must carry calls: {call:?}");
        };
        assert_eq!(rows, &[1, 3]);
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert_eq!(calls[0].tool, "Read");
        assert_eq!(calls[0].target.as_deref(), Some("notes.txt"));
        assert_eq!(calls[0].added, None, "a read cannot add lines");
        assert_eq!(calls[1].tool, "Edited");
        assert_eq!(calls[1].added, Some(1));
        assert_eq!(calls[1].removed, Some(1));
        assert_eq!(calls[1].status, "done");
    }

    #[test]
    fn a_word_before_the_sign_is_not_a_count() {
        // explored 1 file must not become a removal count, and neither must a
        // hyphen inside a word.
        let call = parse_tool_line("♦ Edited a-b.txt, explored 1 file");
        assert_eq!(call.tool, "Edited");
        assert_eq!(call.added, None);
        assert_eq!(call.removed, None);
        assert_eq!(call.target.as_deref(), Some("a-b.txt"));
    }

    #[test]
    fn a_target_ends_before_the_padding_and_the_gutter() {
        // Read off a real frame: the row runs to the screen edge, so the target is
        // followed by padding and then the scrollbar the application draws there.
        let call = parse_tool_line("  ♦ Read notes.txt                                        █");
        assert_eq!(call.target.as_deref(), Some("notes.txt"), "{call:?}");
        assert_eq!(call.tool, "Read");
    }

    #[test]
    fn a_usage_line_becomes_a_meter() {
        // The line as a real frame drew it: the project path, the usage figure, and
        // the label the application puts after its separator.
        let page = page_from_frame(&frame_of(&["~/p 1.3K / 500K │ [Dashboard]"], 60), "s");
        let meter = page
            .create_surface
            .components
            .iter()
            .find(|component| component.component == "Meter")
            .unwrap_or_else(|| panic!("the usage line must be a Meter: {page:?}"));
        let Body::Meter {
            used,
            limit,
            fraction,
            label,
            ..
        } = &meter.body
        else {
            panic!("a Meter component must carry its figures: {meter:?}");
        };
        assert_eq!(used, "1.3K");
        assert_eq!(limit, "500K");
        assert_eq!(label.as_deref(), Some("[Dashboard]"));
        let fraction = fraction.expect("both sides are numbers");
        assert!(
            (fraction - 0.0026).abs() < 1e-3,
            "1.3K of 500K is about 0.0026, not {fraction}"
        );
    }

    #[test]
    fn a_path_is_not_a_meter() {
        // A slash is not enough: a path has words around it, not numbers.
        let page = page_from_frame(&frame_of(&["~/A/L/T/u/proj"], 40), "s");
        assert!(
            page.create_surface
                .components
                .iter()
                .all(|component| component.component != "Meter"),
            "a path must not become a meter: {page:?}"
        );
    }

    #[test]
    fn a_page_this_crate_produces_passes_its_own_audit() {
        let page = page_from_frame(&frame_of(&["hello", "", "  > a prompt", "plain"], 40), "s");
        let problems = audit(&page);
        assert!(
            problems.is_empty(),
            "a produced page must be sound: {problems:?}"
        );
    }

    #[test]
    fn a_dangling_child_and_a_missing_root_are_both_reported() {
        let page = Envelope {
            version: ENVELOPE_VERSION.to_owned(),
            create_surface: Surface {
                surface_id: "bad".to_owned(),
                catalog_id: CATALOGUE_ID.to_owned(),
                root: "nowhere".to_owned(),
                components: vec![Component {
                    id: "root".to_owned(),
                    component: "Column".to_owned(),
                    source: Provenance::Seen,
                    body: Body::Column {
                        children: vec!["ghost".to_owned()],
                    },
                }],
            },
        };
        let problems = audit(&page);
        assert!(
            problems.iter().any(|problem| problem.contains("ghost")),
            "a child that does not exist must be reported: {problems:?}"
        );
        assert!(
            problems.iter().any(|problem| problem.contains("nowhere")),
            "a root that does not exist must be reported: {problems:?}"
        );
    }

    #[test]
    fn a_span_past_the_end_of_its_line_is_reported() {
        let page = Envelope {
            version: ENVELOPE_VERSION.to_owned(),
            create_surface: Surface {
                surface_id: "bad".to_owned(),
                catalog_id: CATALOGUE_ID.to_owned(),
                root: "root".to_owned(),
                components: vec![
                    Component {
                        id: "root".to_owned(),
                        component: "Column".to_owned(),
                        source: Provenance::Seen,
                        body: Body::Column {
                            children: vec!["b".to_owned()],
                        },
                    },
                    Component {
                        id: "b".to_owned(),
                        component: "Block".to_owned(),
                        source: Provenance::Seen,
                        body: Body::Block {
                            role: "text".to_owned(),
                            rows: [0, 1],
                            lines: vec!["ab".to_owned()],
                            tabs: Vec::new(),
                            shortcuts: Vec::new(),
                            properties: Vec::new(),
                            spans: vec![vec![InlineSpan {
                                start: 0,
                                end: 9,
                                style: Emphasis::default(),
                            }]],
                        },
                    },
                ],
            },
        };
        let problems = audit(&page);
        assert!(
            problems.iter().any(|problem| problem.contains("span 0..9")),
            "a span past the line must be reported: {problems:?}"
        );
    }

    #[test]
    fn a_page_turns_back_into_the_screen_it_described() {
        // The lossless property, on a screen with a blank separator, a prompt line and
        // a tool line: what comes back must be the text that went in.
        let frame = frame_of(
            &[
                "Grok Build",
                "",
                "  > hello",
                "  ♦ Read notes.txt",
                "1.3K / 500K │ [Dashboard]",
            ],
            60,
        );
        let page = page_from_frame(&frame, "s");
        let screen = render_text(&page);
        assert_eq!(
            screen,
            vec![
                "Grok Build".to_owned(),
                String::new(),
                "  > hello".to_owned(),
                "  ♦ Read notes.txt".to_owned(),
                "1.3K / 500K │ [Dashboard]".to_owned(),
            ]
        );
    }

    #[test]
    fn a_blank_screen_renders_as_nothing() {
        let page = page_from_frame(&frame_of(&["", ""], 5), "s");
        assert!(render_text(&page).is_empty());
    }

    #[test]
    fn a_page_scored_against_itself_is_perfect() {
        let page = page_from_frame(&frame_of(&["one", "", "two", "three"], 20), "s");
        let score = compare(&page, &page);
        assert!(score.is_perfect(), "{score:?}");
        assert_eq!(score.structure(), 1.0);
        assert_eq!(score.text(), 1.0);
    }

    #[test]
    fn a_split_block_keeps_its_text_and_loses_its_structure() {
        // The failure the two scores exist to tell apart: the same characters, put in
        // two components instead of one.
        let whole = page_from_frame(&frame_of(&["one", "two"], 20), "s");
        let split = page_from_frame(&frame_of(&["one", "", "two"], 20), "s");
        let score = compare(&whole, &split);
        assert_eq!(score.components, 1);
        assert_eq!(
            score.matched, 0,
            "a split block is not the block: {score:?}"
        );
        assert_eq!(score.structure(), 0.0);
        assert!(score.text() < 1.0, "a shifted row is not the same row");
        assert_eq!(score.rows_matched, 1, "the first row still matches");
    }

    #[test]
    fn a_repeated_component_does_not_earn_credit_twice() {
        // Two identical blocks in the answer can only match the one in the reference.
        let reference = page_from_frame(&frame_of(&["only"], 20), "s");
        let mut doubled = reference.clone();
        let block = doubled.create_surface.components[1].clone();
        doubled.create_surface.components.push(block);
        let score = compare(&reference, &doubled);
        assert_eq!(score.matched, 1);
        assert_eq!(score.components, 1);
    }

    #[test]
    fn an_empty_reference_is_a_perfect_match() {
        let blank = page_from_frame(&frame_of(&["", ""], 5), "s");
        assert!(compare(&blank, &blank).is_perfect());
    }

    #[test]
    fn the_input_box_is_named_a_composer() {
        // The three lines every recorded screen of this application shows.
        let page = page_from_frame(
            &frame_of(&["  ╭─────────╭", "  │ > type here │", "  ╰─────────╰"], 24),
            "s",
        );
        let role = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { role, .. } => Some(role.as_str()),
                _ => None,
            })
            .expect("the box must be a block");
        assert_eq!(role, "composer", "{page:?}");
    }

    #[test]
    fn a_rounded_panel_is_a_panel_not_a_composer() {
        // Structure alone is not enough: a box that holds text but no input is a panel.
        let page = page_from_frame(
            &frame_of(&["  ╭─────────╭", "  │ changelog │", "  ╰─────────╰"], 24),
            "s",
        );
        let roles: Vec<&str> = page
            .create_surface
            .components
            .iter()
            .filter_map(|component| match &component.body {
                Body::Block { role, .. } => Some(role.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(roles, vec!["panel"], "{page:?}");
    }

    #[test]
    fn a_panel_reports_the_names_along_its_tab_strip() {
        // The shape a recorded overlay draws: three view names above a rule that spans
        // the panel. Context usage is one name, which is why fields split on wide gaps
        // rather than on whitespace.
        let page = page_from_frame(
            &frame_of(
                &[
                    "  ┌────────────────────┌",
                    "  │ Context usage  Usage limit  Session info │",
                    "  │───────────────────────────│",
                    "  │ Usage limits are managed by your team. │",
                    "  └────────────────────└",
                ],
                60,
            ),
            "s",
        );
        let block = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { tabs, role, .. } => Some((role.clone(), tabs.clone())),
                _ => None,
            })
            .expect("the overlay must be a block");
        assert_eq!(block.0, "panel");
        assert_eq!(
            block.1,
            vec![
                "Context usage".to_owned(),
                "Usage limit".to_owned(),
                "Session info".to_owned()
            ]
        );
    }

    #[test]
    fn prose_without_a_rule_reports_no_tabs() {
        let page = page_from_frame(
            &frame_of(&["Context usage  Usage limit", "and then a sentence"], 40),
            "s",
        );
        let tabs = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { tabs, .. } => Some(tabs.clone()),
                _ => None,
            })
            .expect("the prose must be a block");
        assert!(
            tabs.is_empty(),
            "a rule is what makes a tab strip: {tabs:?}"
        );
    }

    #[test]
    fn the_startup_card_reports_its_shortcut_pairs() {
        // Three rows sharing a right edge, as the real card draws them, inside borders.
        let page = page_from_frame(
            &frame_of(
                &[
                    "│ Resume session                    ctrl+r │",
                    "│ New worktree                      ctrl+w │",
                    "│ Quit                              ctrl+q │",
                ],
                52,
            ),
            "s",
        );
        let pairs = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { shortcuts, .. } => Some(shortcuts.clone()),
                _ => None,
            })
            .expect("the card must be a block");
        assert_eq!(pairs.len(), 3, "{pairs:?}");
        assert_eq!(pairs[0].label, "Resume session");
        assert_eq!(pairs[0].key, "ctrl+r");
        assert_eq!(pairs[2].key, "ctrl+q");
    }

    #[test]
    fn prose_with_a_wide_gap_is_not_a_shortcut_list() {
        // One row cannot share an edge with itself.
        let page = page_from_frame(&frame_of(&["hello there    world"], 40), "s");
        let pairs = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { shortcuts, .. } => Some(shortcuts.clone()),
                _ => None,
            })
            .expect("the prose must be a block");
        assert!(pairs.is_empty(), "{pairs:?}");
    }

    #[test]
    fn a_square_cornered_box_is_still_a_composer() {
        // The same box drawn for a terminal without rounded corners, which is what an
        // application does when it thinks the glyph set has no ╭.
        let page = page_from_frame(
            &frame_of(&["  ┌─────────┌", "  │ > type here │", "  └─────────└"], 24),
            "s",
        );
        let role = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { role, .. } => Some(role.as_str()),
                _ => None,
            })
            .expect("the box must be a block");
        assert_eq!(role, "composer", "{page:?}");
    }

    #[test]
    fn an_overlay_without_a_bottom_border_does_not_swallow_the_composer() {
        // The shape a recorded /help screen has: an overlay whose bottom border is
        // scrolled off, with the input box touching it below. They are two things, and
        // the whole block used to be described as one composer.
        let page = page_from_frame(
            &frame_of(
                &[
                    "  ┌─ Commands ────────┌",
                    "  │ New Session      Ctrl+N │",
                    "  │ Back to Home      /home  │",
                    "  ╭─────────╭",
                    "  │ > type here │",
                    "  ╰─────────╰",
                ],
                30,
            ),
            "s",
        );
        let kinds: Vec<String> = page
            .create_surface
            .components
            .iter()
            .filter(|component| component.component == "Block")
            .map(|component| match &component.body {
                Body::Block { role, rows, .. } => format!("{role}:{}", rows[0]),
                _ => String::new(),
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["text:0".to_owned(), "composer:3".to_owned()],
            "{page:?}"
        );
    }

    #[test]
    fn an_information_panel_reports_its_named_values() {
        // Both shapes the recorded /session-info panel uses: a label with its value
        // on the line beneath, and a label and value on one line.
        let page = page_from_frame(
            &frame_of(
                &[
                    "│ Shell version:          │",
                    "│ 1.0.70                   │",
                    "│ Model: Grok 4.7          │",
                    "│                           │",
                ],
                30,
            ),
            "s",
        );
        let pairs = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { properties, .. } => Some(properties.clone()),
                _ => None,
            })
            .expect("the panel must be a block");
        assert_eq!(pairs.len(), 2, "{pairs:?}");
        assert_eq!(pairs[0].label, "Shell version");
        assert_eq!(pairs[0].value, "1.0.70");
        assert_eq!(pairs[1].label, "Model");
        assert_eq!(pairs[1].value, "Grok 4.7");
    }

    #[test]
    fn prose_with_a_colon_is_not_a_property() {
        let page = page_from_frame(
            &frame_of(&["Note: this is a sentence about the thing and more"], 60),
            "s",
        );
        let pairs = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { properties, .. } => Some(properties.clone()),
                _ => None,
            })
            .expect("the prose must be a block");
        assert_eq!(
            pairs.len(),
            1,
            "a short label with a value still counts: {pairs:?}"
        );
    }

    #[test]
    fn a_windows_path_is_a_value_not_a_property_called_c() {
        // A real overlay reported Working directory: with C:\Users\… beneath it, and
        // the first version read the drive letter as a property's name.
        let page = page_from_frame(
            &frame_of(
                &[
                    "Working directory:",
                    "C:\\Users\\someone\\project",
                    "Turn: 0",
                ],
                60,
            ),
            "s",
        );
        let pairs = page
            .create_surface
            .components
            .iter()
            .find_map(|component| match &component.body {
                Body::Block { properties, .. } => Some(properties.clone()),
                _ => None,
            })
            .expect("the panel must be a block");
        assert_eq!(pairs.len(), 2, "{pairs:?}");
        assert_eq!(pairs[0].label, "Working directory");
        assert_eq!(pairs[0].value, "C:\\Users\\someone\\project");
        assert_eq!(pairs[1].label, "Turn");
    }

    #[test]
    fn a_collapsed_reasoning_line_is_a_thought_and_names_its_duration() {
        // The line a real screen drew when reasoning was raised: ♦ Thought for 3.0s,
        // with the reasoning collapsed and therefore absent from the page.
        let page = page_from_frame(&frame_of(&["  ♦ Thought for 3.0s"], 40), "s");
        let thought = page
            .create_surface
            .components
            .iter()
            .find(|component| component.component == "Thought")
            .unwrap_or_else(|| panic!("the reasoning line must be a Thought: {page:?}"));
        let Body::Thought { seconds, rows, .. } = &thought.body else {
            panic!("a Thought must carry its own shape: {thought:?}");
        };
        assert_eq!(seconds, &Some(3.0));
        assert_eq!(rows, &[0, 1]);
    }

    #[test]
    fn an_ordinary_tool_line_stays_a_tool_call() {
        let page = page_from_frame(&frame_of(&["  ♦ Read notes.txt"], 40), "s");
        assert!(
            page.create_surface
                .components
                .iter()
                .all(|component| component.component != "Thought"),
            "only reasoning is a Thought: {page:?}"
        );
        assert!(
            page.create_surface
                .components
                .iter()
                .any(|component| component.component == "ToolCall"),
            "a read is still a tool call: {page:?}"
        );
    }
}
