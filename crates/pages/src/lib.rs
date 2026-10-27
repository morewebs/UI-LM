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
//! - No `Diff`, `Table`, `Meter`, `ToolCall` or `Thought`: those need per-span
//!   attributes and roles this slice cannot yet infer honestly.
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

use std::collections::HashSet;

use serde::Serialize;
use ui_lm_frames::{decode_grid, Frame};

/// The envelope version this crate writes.
///
/// Pinned independently of the catalogue version: A2UI is pre-1.0, and an envelope
/// bump must not silently move the terminal vocabulary (ADR 0002).
pub const ENVELOPE_VERSION: &str = "v0.9.1";

/// The catalogue the components belong to.
pub const CATALOGUE_ID: &str = "moreweb.terminal/v1";

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
    },
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
        let text = &lines[first..last];
        let id = block_id(text);
        children.push(id.clone());
        components.push(Component {
            id,
            component: "Block".to_owned(),
            // `insert` is the whole provenance rule: the first frame to show this
            // block read it from the cells, every later one carried it forward.
            source: if appeared.insert(block_id(text)) {
                Provenance::Seen
            } else {
                Provenance::Carried
            },
            body: Body::Block {
                role: role_of(text).to_owned(),
                rows: [
                    u16::try_from(first).unwrap_or(u16::MAX),
                    u16::try_from(last).unwrap_or(u16::MAX),
                ],
                lines: text.to_vec(),
            },
        });
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

/// A role, from what the block's own text says about itself.
///
/// Only the two roles the text can prove are named; everything else is `text`
/// rather than a confident guess that would be wrong on the page.
fn role_of(lines: &[String]) -> &'static str {
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
    use ui_lm_frames::frame_from_chars;

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
            }
        );
        assert_eq!(
            blocks[1].body,
            Body::Block {
                role: "text".to_owned(),
                rows: [2, 4],
                lines: vec!["two".to_owned(), "three".to_owned()],
            }
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
                Body::Column { .. } => None,
            })
            .collect();
        assert_eq!(roles, vec!["text", "prompt"]);
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
}
