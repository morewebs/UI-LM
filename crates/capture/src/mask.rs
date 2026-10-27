//! Which of the changes between two frames are noise.
//!
//! A recording of an animated program is full of changes that are not content. A
//! spinner turns, a turn timer ticks, a streaming cursor blinks: the cells move
//! while the application has nothing new to say. Frames that differ only in a
//! spinner glyph are poison to patch training downstream, which is why the spec
//! makes volatile-span masking a rig requirement — and why masking is recorded
//! rather than applied: the cells stay in the frame, and the span says which of
//! them were noise.
//!
//! # The rule
//!
//! A change is volatile when it arrived with nothing to explain it: no event from
//! the application's own stream, no keystroke written to the PTY, and no change of
//! surface between the two frames. That is the whole rule. A flip of the
//! `alt_screen` flag replaces the surface the cells are drawn on, so the new screen
//! is new content whatever it holds, and a flip is never volatile. The obvious
//! alternative — "a change that looks like a spinner" — needs a list of glyphs that
//! a new animation invalidates, and the obvious mechanical alternative, diffing
//! without looking at whether anything happened, gets typeahead echo wrong: a
//! keystroke that has not yet been answered can echo into empty cells with no
//! application event behind it, and a recorder that called that noise would teach a
//! model that typing changes nothing.

use ui_lm_frames::{decode_grid, Frame, MaskReason, Span};

/// What the application did between two frames.
///
/// The two signals are separate because they answer separate questions — did the
/// application report anything, and did the user type anything — and a caller that
/// knew only one of them could not classify: an event without a keystroke is a
/// repaint the application chose, a keystroke without an event is an echo that has
/// not been answered yet, and both are real content.
pub struct Semantics {
    /// The application's own event stream grew between these two frames.
    pub event_between: bool,
    /// A keystroke was written to the PTY between these two frames.
    pub keystroke_between: bool,
}

/// The spans of `next` that changed with nothing to explain the change.
///
/// Everything here is metadata: the cells stay in `next.runs`, and a span says
/// that the cells it covers are known to be noise, not that they may be dropped.
/// A consumer that wants content alone filters on the spans; a consumer reading a
/// recorded session a year later still has the screen that was actually on the
/// terminal.
///
/// Only rows `next` has are described, and a cell the previous frame did not have
/// — a frame of a different shape, or the empty frame before a session's first
/// screen — is a blank, which is how [`changed_rows`](ui_lm_frames::changed_rows)
/// treats a missing row. Spans come out ordered by row and then by column.
///
/// A frame whose `alt_screen` flag differs from the previous frame's yields no
/// spans at all. Entering or leaving the alternate screen replaces the surface, so
/// every cell of the new one is new content — [`changed_rows`](ui_lm_frames::changed_rows)
/// reports every row for a flip for exactly that reason. Without this, a flip that
/// arrived with no event and no keystroke would have the whole new screen masked as
/// noise, which is the failure this module exists to prevent.
pub fn classify(prev: &Frame, next: &Frame, sem: Semantics) -> Vec<Span> {
    if sem.event_between || sem.keystroke_between || prev.alt_screen != next.alt_screen {
        return Vec::new();
    }

    // Both sides are decoded onto `next`'s shape: the frame being described is the
    // new one, and comparison is per cell because a cell is the smallest thing that
    // can differ. A row-level test would call a frame pair identical whenever a row
    // held some content — the widest glyph in a cell, the tail of a wide glyph, a
    // style change on a blank — and masking would then record nothing for changes
    // that really happened.
    let before = decode_grid(&prev.runs, next.rows, next.cols);
    let after = decode_grid(&next.runs, next.rows, next.cols);

    // One maximal run per stretch of differing cells. The walk restarts at every
    // row deliberately: cells at the end of one row and the start of the next are
    // not adjacent on screen, so one span spanning them would describe cells that
    // did not change.
    let mut spans = Vec::new();
    for (row, (before_row, after_row)) in before.iter().zip(&after).enumerate() {
        let mut col = 0;
        while col < after_row.len() {
            if before_row[col] == after_row[col] {
                col += 1;
                continue;
            }
            let start = col;
            while col < after_row.len() && before_row[col] != after_row[col] {
                col += 1;
            }
            spans.push(Span {
                row: row as u16,
                col: start as u16,
                len: (col - start) as u16,
                reason: MaskReason::NoSemanticChange,
            });
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::{classify, Semantics};
    use ui_lm_frames::{decode_grid, encode_grid, frame_from_chars, Frame, MaskReason};

    /// A frame holding `chars`, one row per entry, `cols` cells wide.
    ///
    /// The tests describe screens as characters; this is the same shape the capture
    /// path builds, so a test frame differs from a recorded one only in how it got
    /// its content.
    fn frame(seq: u32, cols: u16, chars: &[Vec<char>]) -> Frame {
        frame_from_chars(seq, chars.len() as u16, cols, chars)
    }

    /// Sets the wide-glyph continuation flag on the cell at `(row, col)`.
    ///
    /// A `Frame` stores runs, so a single cell cannot be flagged in place; decoding
    /// and re-encoding is how a test says "this cell is a wide-glyph tail" without
    /// widening the public API for the sake of one test.
    fn set_wide_tail(frame: &mut Frame, row: u16, col: u16) {
        let mut grid = decode_grid(&frame.runs, frame.rows, frame.cols);
        grid[row as usize][col as usize].wide_tail = true;
        frame.runs = encode_grid(&grid);
    }

    #[test]
    fn spinner_frames_with_no_event_are_masked() {
        let a = frame(1, 8, &[vec!['.', ' ', 'r', 'u', 'n', 'n', 'i', 'n']]);
        let b = frame(2, 8, &[vec!['*', ' ', 'r', 'u', 'n', 'n', 'i', 'n']]);
        let spans = classify(
            &a,
            &b,
            Semantics {
                event_between: false,
                keystroke_between: false,
            },
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].reason, MaskReason::NoSemanticChange);
        assert_eq!((spans[0].col, spans[0].len), (0, 1));
    }

    #[test]
    fn keystroke_echo_is_not_masked() {
        let a = frame(0, 8, &[vec![' '; 8]]);
        let b = frame(1, 8, &[vec!['h', 'i', ' ', ' ', ' ', ' ', ' ', ' ']]);
        let spans = classify(
            &a,
            &b,
            Semantics {
                event_between: false,
                keystroke_between: true,
            },
        );
        assert!(
            spans.is_empty(),
            "typeahead echo is real content, not noise"
        );
    }

    #[test]
    fn change_with_an_event_is_not_masked() {
        let a = frame(0, 4, &[vec!['a'; 4]]);
        let b = frame(1, 4, &[vec!['b'; 4]]);
        let spans = classify(
            &a,
            &b,
            Semantics {
                event_between: true,
                keystroke_between: false,
            },
        );
        assert!(spans.is_empty());
    }

    #[test]
    fn both_signals_at_once_are_not_masked() {
        // The rule is a disjunction, so the one combination the other tests do not
        // reach has to be pinned too: two reasons for a change cannot add up to no
        // reason for it.
        let a = frame(0, 3, &[vec!['a'; 3]]);
        let b = frame(1, 3, &[vec!['b'; 3]]);
        let spans = classify(
            &a,
            &b,
            Semantics {
                event_between: true,
                keystroke_between: true,
            },
        );
        assert!(spans.is_empty());
    }

    #[test]
    fn unchanged_frames_produce_no_spans() {
        let a = frame(0, 4, &[vec!['a'; 4]]);
        let spans = classify(
            &a,
            &a,
            Semantics {
                event_between: false,
                keystroke_between: false,
            },
        );
        assert!(spans.is_empty());
    }

    #[test]
    fn a_wide_glyph_tail_difference_is_a_real_change_not_noise() {
        // Same characters, same styles: the ONLY difference is the wide_tail flag
        // on column 1. A row-level "did anything change" test would miss it; a
        // cell-level comparison reports it. Masking must never eat a geometry
        // change.
        let a = frame(1, 3, &[vec!['\u{4f60}', ' ', 'x']]);
        let mut b = a.clone();
        set_wide_tail(&mut b, 0, 1);
        let spans = classify(
            &a,
            &b,
            Semantics {
                event_between: false,
                keystroke_between: false,
            },
        );
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].col, spans[0].len), (1, 1));
    }

    #[test]
    fn an_alt_screen_flip_is_not_masked() {
        // Entering or leaving the alternate screen replaces the surface, so the
        // cells the new surface carries are new content even where they hold what
        // the old one held — `changed_rows` reports every row for a flip for the
        // same reason. A flip that arrived with no event and no keystroke would
        // otherwise have every changed cell masked as noise: masking eating the
        // content it exists to protect. The cells differ on their own here, and
        // the no-flip pair is asserted to produce a span, so the empty result
        // cannot come from a vacuous diff.
        let a = frame(0, 4, &[vec!['a'; 4]]);
        let mut flipped = frame(1, 4, &[vec!['b'; 4]]);
        let plain = flipped.clone();
        flipped.alt_screen = true;

        let quiet = || Semantics {
            event_between: false,
            keystroke_between: false,
        };
        assert!(
            !classify(&a, &plain, quiet()).is_empty(),
            "these two frames' cells do differ; only the flip suppresses the spans"
        );
        assert!(
            classify(&a, &flipped, quiet()).is_empty(),
            "a surface flip is real content, not noise"
        );
    }

    #[test]
    fn spans_are_maximal_runs_inside_one_row() {
        // A span is one maximal run of differing cells *within a row*: two changed
        // stretches with an untouched cell between them are two spans, and a change
        // that reaches the right edge of a row does not continue into the row
        // below. Both mistakes are easy to make by walking the screen as one flat
        // sequence of cells, and both would hand a consumer a span describing cells
        // that did not change.
        let a = frame(
            0,
            4,
            &[vec!['a'; 4], vec!['a'; 4], vec!['b'; 4], vec!['c'; 4]],
        );
        let b = frame(
            1,
            4,
            &[
                vec!['a', 'a', 'X', 'X'],
                vec!['Y', 'Y', 'a', 'a'],
                vec!['b', 'Z', 'b', 'W'],
                vec!['V', 'V', 'c', 'c'],
            ],
        );
        let spans = classify(
            &a,
            &b,
            Semantics {
                event_between: false,
                keystroke_between: false,
            },
        );
        let found: Vec<(u16, u16, u16)> = spans.iter().map(|s| (s.row, s.col, s.len)).collect();
        assert_eq!(
            found,
            vec![(0, 2, 2), (1, 0, 2), (2, 1, 1), (2, 3, 1), (3, 0, 2)],
            "contiguous cells merge, a gap splits, and no span crosses a row edge"
        );
    }
}
