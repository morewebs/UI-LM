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
/// `acp_event_range` names the byte range of the application's event stream that
/// this frame sits at the end of, when the tap could find that stream. Whether the
/// program reported anything between two frames is not a separate input: a range
/// that grew says it did, a range that did not says it did not, and no range at all
/// means nobody can say — which the mask must read as "assume it did".
pub fn frame_from_screen(
    screen: &vt100::Screen,
    seq: u32,
    previous: Option<&Frame>,
    keystroke_between: bool,
    acp_event_range: Option<(u64, u64)>,
    at_ms: Option<u64>,
    event_kinds: Vec<String>,
) -> Frame {
    let event_between = match acp_event_range {
        Some((from, to)) => to > from,
        None => true,
    };
    let (rows, cols) = screen.size();
    let grid = grid_from_screen(screen, cols);
    let mut frame = Frame::empty(rows, cols);
    frame.seq = seq;
    frame.acp_event_range = acp_event_range;
    frame.at_ms = at_ms;
    frame.event_kinds = event_kinds;
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
    // `event_between` says whether the application reported anything between them,
    // which the recorder learns by watching its event stream grow (see
    // [`crate::events`]). It is passed as `true` whenever that stream cannot be
    // found: `true` means "an event may explain this change", the only value that
    // asserts nothing nobody can know. `false` would claim the program said
    // nothing, and every change it made of its own accord would then be recorded as
    // noise, which is the failure the mask exists to prevent. Where the stream is
    // available, `volatile` starts to appear; where it is not, sessions record an
    // empty mask exactly as they did before the tap existed.
    frame.volatile = match previous {
        Some(before) => classify(
            before,
            &frame,
            Semantics {
                event_between,
                keystroke_between,
            },
        ),
        None => Vec::new(),
    };
    // Encoding failures are marked whatever the events say: a replacement character is
    // not content the application drew, and it is not the application reporting anything
    // either. A first frame gets the same treatment, because a first screen full of
    // replacement characters is exactly as unreadable as a later one.
    frame
        .volatile
        .extend(crate::mask::unrepresentable_spans(&frame));
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
        let primary = frame_from_screen(parser.screen(), 0, None, false, None, None, Vec::new());
        assert!(
            !primary.alt_screen,
            "a screen nothing has switched is the primary screen"
        );
        assert_eq!(text(&primary), vec!["ab  ", "cd  ", "    "]);

        // Entering the alternate screen: the same cells, a different surface.
        parser.process(b"\x1b[?1049h");
        let alternate = frame_from_screen(
            parser.screen(),
            1,
            Some(&primary),
            false,
            None,
            None,
            Vec::new(),
        );
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
        let back = frame_from_screen(
            parser.screen(),
            2,
            Some(&alternate),
            false,
            None,
            None,
            Vec::new(),
        );
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
        let frame = frame_from_screen(parser.screen(), 0, None, false, None, None, Vec::new());
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
        let first = frame_from_screen(parser.screen(), 0, None, false, None, None, Vec::new());
        let parser = parser_with(2, 2, b"ab");
        let second = frame_from_screen(
            parser.screen(),
            1,
            Some(&first),
            true,
            None,
            None,
            Vec::new(),
        );
        assert_eq!(second.changed_rows, vec![0]);
        assert!(
            second.volatile.is_empty(),
            "an unknown event stream means an unexplained change is content, not noise"
        );
    }

    #[test]
    fn a_grown_event_range_is_what_stops_a_change_being_masked() {
        // The derivation `frame_from_screen` now owns, and the one the mask rests on:
        // the *same* screen change is content when the event stream grew behind it and
        // noise when it did not. Neither frame carries a keystroke, so the event range
        // is the only signal that can separate them.
        let parser = parser_with(2, 2, b"a/");
        let first = frame_from_screen(
            parser.screen(),
            0,
            None,
            false,
            Some((0, 10)),
            Some(0),
            Vec::new(),
        );

        // A carriage return, then a slash over the first cell: the screen changes by
        // one glyph, which is what a repainting status area does.
        let mut parser = parser;
        parser.process(b"\r/");

        let reported = frame_from_screen(
            parser.screen(),
            1,
            Some(&first),
            false,
            Some((10, 20)),
            Some(150),
            vec!["agent_message".to_owned()],
        );
        assert!(
            reported.volatile.is_empty(),
            "a change the program reported must not be masked: {:?}",
            reported.volatile
        );
        assert_eq!(reported.event_kinds, vec!["agent_message".to_owned()]);

        let silent = frame_from_screen(
            parser.screen(),
            2,
            Some(&first),
            false,
            Some((10, 10)),
            Some(300),
            Vec::new(),
        );
        assert!(
            !silent.volatile.is_empty(),
            "a change with no event behind it is the noise the mask exists to mark"
        );
    }
}
