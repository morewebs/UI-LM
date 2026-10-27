//! The emulator's screen turned into a frame: the one place a captured screen is
//! described.
//!
//! This is where the pieces meet. The grid comes from the emulator, the
//! `changed_rows` from the frame model's row-by-row diff, the volatile spans from
//! the mask, and the numbers come from the caller. Everything a recorded frame says
//! about a screen is decided here, so a fact about the screen has exactly one place
//! it can be dropped — and the tests can feed a screen straight in rather than
//! inferring what it would have produced from a session on disk.
//!
//! It is a function of a `vt100::Screen` rather than of a [`crate::Driver`] so that
//! the screen-to-frame step can be exercised without a child process behind it. A
//! real child is still what the end-to-end tests use; what they cannot show is a
//! screen the pty host will not forward (see [`frame_from_screen`]'s note on the
//! alternate screen).

use ui_lm_frames::{changed_rows, encode_grid, grid_from_screen, Frame};

use crate::mask::{classify, Semantics};

/// Builds the frame for the screen as it now stands.
///
/// The first frame is emitted **unclassified**, because there is no frame before it.
/// Passing `Frame::empty` to `classify` would be worse than useless: nothing has
/// been logged yet, so every cell of the first screen would be reported as a change
/// with no event behind it and the whole first screen would be masked as noise — the
/// one thing masking must never do. `Frame::empty` is still the right *diff*
/// baseline, because every row of a first screen really is new content.
///
/// `alt_screen` is read from the emulator rather than inferred from the cells: the
/// alternate screen is a different surface, and two frames can hold identical cells
/// while one of them was drawn on a surface the other was not (which is why
/// `changed_rows` reports every row when the flag changes).
///
/// `acp_event_range` is left `None`. The application's event stream is not wired
/// into the recording path, so no frame can honestly name the events it drew; the
/// field is on the frame so the next plan fills it without a second format change.
pub fn frame_from_screen(
    screen: &vt100::Screen,
    seq: u32,
    previous: Option<&Frame>,
    keystroke_between: bool,
) -> Frame {
    let (rows, cols) = screen.size();
    let grid = grid_from_screen(screen, cols);
    let mut frame = Frame::empty(rows, cols);
    frame.seq = seq;
    frame.alt_screen = screen.alternate_screen();
    frame.cursor = if screen.hide_cursor() {
        None
    } else {
        Some(screen.cursor_position())
    };
    frame.runs = encode_grid(&grid);
    frame.changed_rows = match previous {
        Some(before) => changed_rows(before, &frame),
        None => changed_rows(&Frame::empty(rows, cols), &frame),
    };
    // Volatile spans, from the two signals the mask rule needs.
    //
    // `keystroke_between` is known exactly: the record loop is the only thing that
    // writes to the child, and it counts the keystrokes it queued between the two
    // frames.
    //
    // `event_between` is **not** known. The application's own event stream is not
    // wired into the recording path yet, so this cannot say whether the program
    // reported anything between these two frames. It is passed as `true` —
    // "assume an event may explain this change" — because that is the only value
    // that does not assert a fact nobody can know. `false` would claim the
    // application reported nothing, and every change the program made of its own
    // accord would then be recorded as noise: masking eating content is precisely
    // the failure the mask exists to prevent. The cost of `true` is the other
    // direction — nothing is masked until the event stream arrives, so `volatile` is
    // empty in every frame of every session recorded so far — and that cost buys a
    // session that has not mislabelled any content. When the event stream is wired,
    // this becomes the real count of events between the two frames and the spans
    // start to appear.
    frame.volatile = match previous {
        Some(before) => classify(
            before,
            &frame,
            Semantics {
                event_between: true,
                keystroke_between,
            },
        ),
        None => Vec::new(),
    };
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_lm_frames::decode_grid;

    /// A parser of `rows x cols` with `bytes` already fed to it.
    fn parser_with(rows: u16, cols: u16, bytes: &[u8]) -> vt100::Parser {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(bytes);
        parser
    }

    /// The characters a frame holds, one line per row.
    fn text(frame: &Frame) -> Vec<String> {
        decode_grid(&frame.runs, frame.rows, frame.cols)
            .iter()
            .map(|row| row.iter().map(|cell| cell.ch).collect::<String>())
            .collect()
    }

    #[test]
    fn a_screen_the_emulator_calls_alternate_is_a_frame_that_says_so() {
        // Item from the final review: `screen.alternate_screen()` reaching
        // `frame.alt_screen` had no test, and the 35 real sessions produced no
        // alternate-screen frame at all. The real-session half cannot be shown on
        // Windows (ConPTY consumes the switch before the client sees it — see the
        // note in `tests/cli.rs`), so the composition is proved here, on the same
        // emulator the recorder uses, and end-to-end by
        // `tests/cli.rs::a_child_that_switches_to_the_alternate_screen_is_recorded`.
        let mut parser = parser_with(3, 4, b"ab\r\ncd");
        let primary = frame_from_screen(parser.screen(), 0, None, false);
        assert!(
            !primary.alt_screen,
            "a screen nothing has switched is the primary screen"
        );
        assert_eq!(text(&primary), vec!["ab  ", "cd  ", "    "]);

        // Entering the alternate screen: the same cells, a different surface.
        parser.process(b"\x1b[?1049h");
        let alternate = frame_from_screen(parser.screen(), 1, Some(&primary), false);
        assert!(
            alternate.alt_screen,
            "the emulator says this screen is the alternate one, so the frame must too"
        );
        assert_eq!(
            alternate.changed_rows,
            vec![0, 1, 2],
            "entering the alternate screen replaces the whole surface, so every row is new \
             content even where it now holds nothing"
        );
        assert_eq!(
            text(&alternate),
            vec!["    ", "    ", "    "],
            "the alternate surface starts blank, whatever the primary screen held"
        );

        // And back out: the primary screen is restored, and the flag goes back with
        // it rather than sticking to the last thing the emulator was told.
        parser.process(b"\x1b[?1049l");
        let back = frame_from_screen(parser.screen(), 2, Some(&alternate), false);
        assert!(
            !back.alt_screen,
            "leaving the alternate screen is recorded too"
        );
        assert_eq!(back.changed_rows, vec![0, 1, 2]);
        assert_eq!(
            text(&back),
            vec!["ab  ", "cd  ", "    "],
            "the primary surface came back as it was left"
        );
    }

    #[test]
    fn a_frame_name_no_event_range_until_an_event_stream_exists() {
        let parser = parser_with(2, 2, b"a");
        let frame = frame_from_screen(parser.screen(), 0, None, false);
        assert_eq!(
            frame.acp_event_range, None,
            "no event stream is wired in, so no frame may claim a range of one"
        );
    }

    #[test]
    fn a_frame_for_a_quiet_screen_masks_nothing() {
        // `event_between: true` while no event stream exists means nothing is ever
        // masked: the empty `volatile` of every recorded session is explained here so
        // that it is a decision rather than a mystery.
        let parser = parser_with(2, 2, b"a");
        let first = frame_from_screen(parser.screen(), 0, None, false);
        let parser = parser_with(2, 2, b"ab");
        let second = frame_from_screen(parser.screen(), 1, Some(&first), true);
        assert_eq!(second.changed_rows, vec![0]);
        assert!(
            second.volatile.is_empty(),
            "an unknown event stream means an unexplained change is content, not noise"
        );
    }
}
