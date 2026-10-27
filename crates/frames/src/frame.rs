//! The frame: one settled screen, plus what can be said about it without looking
//! at the previous one.
//!
//! A frame is what the rest of the rig consumes. The session writer appends it to
//! JSONL, the validator diffs it against the application's own draw buffer, and a
//! trainer reads only the rows whose content changed since the frame before. So
//! the frame carries both the screen (as runs) and the two pieces of metadata that
//! make the screen usable as training input: which rows moved, and which of those
//! moves were noise.

use serde::{Deserialize, Serialize};

use crate::grid::{Grid, StyledCell};
use crate::rle::{encode_grid, CellRun};

/// One settled screen, numbered.
///
/// The cells are runs rather than a grid because that is the form every consumer
/// wants: the session file stores them, the diff walks them row by row, and a
/// comparison against the application's own buffer decodes them once. Serialising
/// the runs also means a recorded frame is exactly what the codec can represent —
/// there is no second shape for a frame to drift into.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Position of this frame in its session, counting from zero.
    pub seq: u32,
    /// Screen height, in rows.
    pub rows: u16,
    /// Screen width, in columns.
    pub cols: u16,
    /// True when the application had switched to the terminal's alternate screen.
    ///
    /// The alternate screen is a different surface, not more of the same one: a
    /// full-screen application enters it to draw and leaves it to restore what was
    /// underneath. A frame recorded without this flag cannot be told apart from a
    /// primary-screen frame that happens to look the same, and the screen it was
    /// drawn on is part of what was captured.
    pub alt_screen: bool,
    /// Where the cursor was, as `(row, col)`, when the terminal shows one.
    pub cursor: Option<(u16, u16)>,
    /// The screen itself, run-length encoded, covering every cell of every row.
    pub runs: Vec<CellRun>,
    /// Rows of this frame whose content differs from the previous frame's.
    ///
    /// Always ascending, and always every row for the first frame of a session:
    /// there is no previous screen, so nothing on this one is "unchanged".
    pub changed_rows: Vec<u16>,
    /// Spans of this frame's changes that were classified as noise.
    ///
    /// The cells stay in `runs`. A masked span is metadata about a change, not
    /// permission to drop the content that changed.
    pub volatile: Vec<Span>,
    /// The stretch of the application's event stream this frame's changes came
    /// from, as a half-open `(first, end)` range of event indexes.
    ///
    /// §7.1 names it in the frame record, and it is what label alignment needs: a
    /// block of the transcript can only be tied to a *frame* if the frame says which
    /// events it drew. It is `None` in this plan — the recorder has no event stream
    /// wired into it yet, so there is no range it could honestly name, and a guessed
    /// one would be a label pointing at a screen it may not describe. The field
    /// exists now so the next plan fills it without changing the session format a
    /// second time; a frame written without it reads back as `None`.
    #[serde(default)]
    pub acp_event_range: Option<(u64, u64)>,
    /// How long after the session started this frame was taken, in milliseconds.
    ///
    /// Recorded because the latency the plan measures is the time from the
    /// application drawing to the recorder having a frame, and that cannot be
    /// reconstructed afterwards: the earlier capture could only approximate it by
    /// polling the file's length every 100 ms and saying so. `None` in a session
    /// written before this field existed.
    #[serde(default)]
    pub at_ms: Option<u64>,
    /// The kinds of application event behind this frame's changes, in order.
    ///
    /// `acp_event_range` says *where* in the stream the frame sits; this says
    /// *what* was in it, as the application labelled it (`agent_message`,
    /// `tool_call`, `hook_execution`, …). A frame that holds a tool-call block and
    /// names `tool_call` is a labelled example; a frame that holds one and names
    /// nothing is an unlabelled screen. Empty when no event stream was reachable,
    /// which is a different statement from "nothing happened" and is recorded as
    /// the former.
    #[serde(default)]
    pub event_kinds: Vec<String>,
}

/// A contiguous group of cells that changed for a reason other than the
/// application drawing something new.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// Row the span is on.
    pub row: u16,
    /// Column the span starts at.
    pub col: u16,
    /// How many cells the span covers.
    pub len: u16,
    /// Why those cells are considered volatile.
    pub reason: MaskReason,
}

/// Why a span was masked.
///
/// The variants travel to the session file as words (`no_semantic_change`), so the
/// reason a span was masked is readable a year later without this source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskReason {
    /// The cells changed while the application's own event stream did not move:
    /// a spinner turned, a clock ticked, a progress bar repainted, and no state
    /// the application considered worth reporting is behind it.
    NoSemanticChange,
    /// A glyph this rig knows renders differently than the emulator reports, so
    /// its cells are recorded but not compared.
    KnownGlyph,
}

impl Frame {
    /// A `rows x cols` frame with no content and no history.
    ///
    /// This is the frame before the first frame: a session diffs its first real
    /// frame against this, and since this has no runs at all, every row of that
    /// frame differs from it and is reported as changed. That is the only input
    /// shape that lets a consumer reconstruct a first screen from a diff.
    pub fn empty(rows: u16, cols: u16) -> Self {
        Self {
            seq: 0,
            rows,
            cols,
            alt_screen: false,
            cursor: None,
            runs: Vec::new(),
            changed_rows: Vec::new(),
            volatile: Vec::new(),
            acp_event_range: None,
            at_ms: None,
            event_kinds: Vec::new(),
        }
    }
}

/// A frame built from a grid of character literals, every cell in the default
/// style.
///
/// Tests describe a screen as its characters, and so does the validator when it
/// compares against the application's draw buffer. This is the one place that
/// turns such a description into the runs a captured screen would produce, so
/// there is one definition of "a frame holding these characters" rather than one
/// per test file.
///
/// `chars` may hold fewer rows than `rows`, and any row may be shorter than
/// `cols`: the missing cells are blanks, so the frame is always exactly the
/// declared shape.
pub fn frame_from_chars(seq: u32, rows: u16, cols: u16, chars: &[Vec<char>]) -> Frame {
    let mut frame = Frame::empty(rows, cols);
    frame.seq = seq;
    frame.runs = encode_grid(&grid_from_chars(rows, cols, chars));
    frame
}

/// The character grid a frame built from literals holds, as a full grid.
fn grid_from_chars(rows: u16, cols: u16, chars: &[Vec<char>]) -> Grid {
    let mut grid = vec![vec![StyledCell::blank(); cols as usize]; rows as usize];
    for (row, line) in chars.iter().take(rows as usize).enumerate() {
        for (col, &ch) in line.iter().take(cols as usize).enumerate() {
            grid[row][col] = StyledCell::new(ch);
        }
    }
    grid
}

/// The rows of `next` whose content differs from the same row of `prev`, in
/// ascending order.
///
/// The comparison is row by row: a row is compared as the runs that belong to it,
/// so it is that row's content being compared and the row it sits at deciding
/// whether that content is a change. Comparing the two frames as whole run vectors
/// gets a scroll exactly wrong — every line moved up one row, so the new runs are
/// the old runs at different rows — and a training input that reports a scrolled
/// screen as unchanged is the one input that teaches nothing.
///
/// Only rows `next` has are reported; a row `prev` had and `next` does not is not
/// a row of the frame being described. A row with no runs compares as empty, which
/// is why a frame with no content at all differs from every row of a real one.
///
/// A frame whose `alt_screen` flag differs from the previous frame's reports every
/// row, whatever the runs say. Entering or leaving the alternate screen replaces
/// the surface, so the cells the new surface carries are new content even where
/// they hold what the old one held; a run diff of a typical flip reports most rows
/// changed, and "most" is not a guarantee. The flag is the evidence that the whole
/// surface changed, and no consumer downstream can recover rows a diff omitted.
pub fn changed_rows(prev: &Frame, next: &Frame) -> Vec<u16> {
    if prev.alt_screen != next.alt_screen {
        return (0..next.rows).collect();
    }
    (0..next.rows)
        .filter(|&row| !row_runs(&prev.runs, row).eq(row_runs(&next.runs, row)))
        .collect()
}

/// The runs belonging to `row`, in the order the encoder wrote them.
fn row_runs(runs: &[CellRun], row: u16) -> impl Iterator<Item = &CellRun> + '_ {
    runs.iter().filter(move |run| run.row == row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_rows_reports_only_rows_that_differ() {
        let a = frame_from_chars(0, 2, 4, &[vec!['a'; 4], vec!['b'; 4]]);
        let b = frame_from_chars(1, 2, 4, &[vec!['a'; 4], vec!['c'; 4]]);
        assert_eq!(changed_rows(&a, &b), vec![1]);
    }

    #[test]
    fn first_frame_reports_every_row_as_changed() {
        let line = vec![' '; 2];
        let a = frame_from_chars(0, 3, 2, &[line.clone(), line.clone(), line]);
        assert_eq!(changed_rows(&Frame::empty(3, 2), &a), vec![0, 1, 2]);
    }

    #[test]
    fn a_scrolled_frame_is_not_reported_as_unchanged() {
        // Review Focus 3. Scrolling shifts every line up one row, so the new frame
        // holds runs the old frame also held, at different rows. A diff that
        // compared runs without their row — as a set, say — calls that unchanged,
        // and a training input that says nothing moved is the one input that
        // teaches nothing. Every row of a scrolled screen holds new content.
        let a = frame_from_chars(0, 2, 3, &[vec!['a'; 3], vec!['b'; 3]]);
        let b = frame_from_chars(1, 2, 3, &[vec!['b'; 3], vec!['c'; 3]]);
        assert_eq!(changed_rows(&a, &b), vec![0, 1]);

        // The same two lines, swapped. Every run of `a` exists in `b` with the same
        // content, so a diff that forgot which row a run belongs to would find
        // nothing to report, and the screen a reader is shown would be the one
        // before the swap.
        let swapped = frame_from_chars(2, 2, 3, &[vec!['b'; 3], vec!['a'; 3]]);
        assert_eq!(changed_rows(&a, &swapped), vec![0, 1]);
    }

    #[test]
    fn a_frame_from_the_alternate_screen_says_so_after_a_round_trip() {
        // Review Focus 3, the other half: a frame drawn on the alternate screen is
        // not the same screen as one drawn on the primary screen, and a frame that
        // loses the flag on its way to the session file cannot be told apart from
        // one that never left it. The flag has to reach the JSONL and come back.
        let mut f = frame_from_chars(0, 1, 3, &[vec!['a', 'b', 'c']]);
        assert!(
            !f.alt_screen,
            "a frame is on the primary screen unless something says otherwise"
        );
        f.alt_screen = true;

        let json = serde_json::to_string(&f).expect("a frame serialises");
        let on_wire: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(
            on_wire["alt_screen"], true,
            "the flag must be a field of its own, not a derived fact"
        );
        let back: Frame = serde_json::from_str(&json).expect("a frame deserialises");
        assert!(back.alt_screen);
    }

    #[test]
    fn an_alternate_screen_flag_change_reports_every_row_as_changed() {
        // Review Focus 3, the frame class that poisons idempotence training.
        // Entering or leaving the alternate screen swaps the whole surface, so
        // every row of the new frame is new content even where a cell happens to
        // hold what the old surface held. A run diff of a real flip reports most
        // rows; "most" is not all, and the flag is exactly the evidence that the
        // whole surface changed rather than part of it.
        let lines = [vec!['a'; 2], vec!['b'; 2], vec!['c'; 2]];
        let mut primary = frame_from_chars(0, 3, 2, &lines);
        let mut alternate = frame_from_chars(1, 3, 2, &lines);

        // Same flag, same cells: nothing moved.
        assert_eq!(
            changed_rows(&primary, &alternate),
            Vec::<u16>::new(),
            "identical frames on the same screen differ in nothing"
        );

        // The cells are still identical; only the surface differs.
        alternate.alt_screen = true;
        assert_eq!(changed_rows(&primary, &alternate), vec![0, 1, 2]);

        // And the other direction, from the alternate screen back to the primary.
        primary.alt_screen = true;
        alternate.alt_screen = false;
        assert_eq!(changed_rows(&primary, &alternate), vec![0, 1, 2]);
    }

    #[test]
    fn a_frame_written_before_acp_event_range_existed_still_reads() {
        // Every frame of the 35 W1 sessions was written without this field: it is a
        // version-1 frame that says nothing about the event stream, and it must read
        // back as saying nothing rather than as an unreadable line. The reader skips
        // an unreadable line with a warning, so a missing default here would not
        // fail — it would quietly shrink every session in the set.
        let mut f = frame_from_chars(0, 1, 2, &[vec!['a', 'b']]);
        assert_eq!(
            f.acp_event_range, None,
            "no event stream is wired in, so no frame names a range"
        );
        let json = serde_json::to_string(&f).expect("a frame serialises");
        let without = {
            let mut value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
            value
                .as_object_mut()
                .expect("a frame is an object")
                .remove("acp_event_range");
            assert!(
                value.get("acp_event_range").is_none(),
                "the field must be gone for this to be an older frame"
            );
            serde_json::to_string(&value).expect("the trimmed frame serialises")
        };
        let back: Frame = serde_json::from_str(&without).expect("an older frame still reads");
        assert_eq!(
            back.acp_event_range, None,
            "a frame without the field says nothing about the event stream"
        );
        assert_eq!(back.runs, f.runs, "and its screen is unchanged");

        // When the event stream arrives, the range is a value of its own and it
        // survives the trip to the file: `(first, end)`, in stream order.
        f.acp_event_range = Some((3, 7));
        let json = serde_json::to_string(&f).expect("a frame serialises");
        let back: Frame = serde_json::from_str(&json).expect("a frame deserialises");
        assert_eq!(back.acp_event_range, Some((3, 7)));
    }

    #[test]
    fn mask_reasons_travel_as_words_rather_than_variant_names() {
        // Spans are written to the session file next to the frames they describe,
        // so the reason's spelling is part of the session format, not an internal
        // detail of one build.
        assert_eq!(
            serde_json::to_string(&MaskReason::NoSemanticChange).expect("serialises"),
            r#""no_semantic_change""#
        );
        assert_eq!(
            serde_json::to_string(&MaskReason::KnownGlyph).expect("serialises"),
            r#""known_glyph""#
        );
    }
}
