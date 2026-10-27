//! When a screen has settled, and which frame number it gets.
//!
//! A program draws by writing bytes, and a screen caught mid-draw is neither the
//! state before the repaint nor the state after it — it is a half-drawn picture
//! that exists nowhere a user could ever have seen it. So a frame is taken when
//! output has stopped for long enough that what is on screen is what the program
//! finished saying, and the frame number says which settled screen this is. The
//! frame model itself lives in `ui_lm_frames::frame`.

use std::time::{Duration, Instant};

/// Decides when output has settled enough for the screen to be a frame.
///
/// A burst of output is many reads: one repaint, one keystroke's echo, one line of
/// log written over several writes. The screen between two of those reads is a
/// state the program never meant to show anyone, so quiescence — not the arrival
/// of bytes — is what separates one frame from the next. [`Quiescer::observe`]
/// says when the current burst has settled; [`Quiescer::take_seq`] numbers the
/// frame that follows.
pub struct Quiescer {
    /// How long output must be quiet before the screen counts as settled.
    pub gap: Duration,
    /// When bytes were last seen. `None` until the first byte arrives.
    last_bytes: Option<Instant>,
    /// Whether the quiet stretch since `last_bytes` has already been reported.
    settled: bool,
    /// The number the next emitted frame takes.
    seq: u32,
}

impl Quiescer {
    /// A quiescer that calls a screen settled after `gap` without output.
    pub fn new(gap: Duration) -> Self {
        Self {
            gap,
            last_bytes: None,
            settled: false,
            seq: 0,
        }
    }

    /// Records that `bytes_read` bytes arrived at `now`, and reports whether the
    /// screen has settled and should be framed.
    ///
    /// True means both: nothing has arrived for at least `gap`, *and* this quiet
    /// stretch has not been reported yet. The second half is what keeps a polling
    /// caller honest. A recorder polls in a loop, so the same quiet stretch is
    /// observed over and over; if every one of those polls reported a settled
    /// screen, an idle session would write one identical frame per poll and each
    /// frame would diff against a copy of itself, leaving `changed_rows` empty
    /// exactly where it should be full. One settled burst is one frame, until new
    /// bytes restart the clock.
    ///
    /// Quiet before any output has ever arrived reports false: there is no screen
    /// yet to frame, and a program that draws nothing should produce no frames.
    pub fn observe(&mut self, now: Instant, bytes_read: usize) -> bool {
        if bytes_read > 0 {
            self.last_bytes = Some(now);
            self.settled = false;
            return false;
        }
        if self.settled {
            return false;
        }
        let Some(last) = self.last_bytes else {
            return false;
        };
        if now.saturating_duration_since(last) < self.gap {
            return false;
        }
        self.settled = true;
        true
    }

    /// The sequence number for the frame being emitted now, counting from zero.
    pub fn take_seq(&mut self) -> u32 {
        let seq = self.seq;
        self.seq += 1;
        seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_burst_settles_once_not_once_per_byte() {
        let mut q = Quiescer::new(Duration::from_millis(150));
        let t0 = Instant::now();
        for i in 0..50 {
            // 50 bytes in 5 ms: one burst, one frame.
            assert!(!q.observe(t0 + Duration::from_millis(i / 10), 1));
        }
        assert!(q.observe(t0 + Duration::from_millis(200), 0));
    }

    #[test]
    fn quiescence_requires_the_full_gap() {
        let mut q = Quiescer::new(Duration::from_millis(150));
        let t0 = Instant::now();
        q.observe(t0, 10);
        assert!(!q.observe(t0 + Duration::from_millis(149), 0));
        assert!(q.observe(t0 + Duration::from_millis(150), 0));
    }

    #[test]
    fn sequence_numbers_increment_per_emitted_frame() {
        let mut q = Quiescer::new(Duration::from_millis(150));
        assert_eq!(q.take_seq(), 0);
        assert_eq!(q.take_seq(), 1);
    }

    #[test]
    fn a_settled_screen_is_not_emitted_again_until_output_resumes() {
        // A recorder polls in a loop, so the same 150 ms of quiet is observed many
        // times over. If each of those polls reported a settled screen, an idle
        // session would write one identical frame per poll, and the frame after a
        // repaint would diff against a copy of itself: every `changed_rows` empty.
        // One settled screen is one frame, until bytes arrive again.
        let mut q = Quiescer::new(Duration::from_millis(150));
        let t0 = Instant::now();
        q.observe(t0, 5);
        assert!(q.observe(t0 + Duration::from_millis(150), 0));
        assert!(
            !q.observe(t0 + Duration::from_millis(200), 0),
            "still quiet: this screen has already been framed"
        );
        assert!(!q.observe(t0 + Duration::from_millis(1000), 0));

        q.observe(t0 + Duration::from_millis(1200), 3);
        assert!(
            !q.observe(t0 + Duration::from_millis(1300), 0),
            "new output restarts the clock"
        );
        assert!(q.observe(t0 + Duration::from_millis(1350), 0));
    }

    #[test]
    fn quiet_before_any_output_is_not_a_frame() {
        let mut q = Quiescer::new(Duration::from_millis(150));
        let t0 = Instant::now();
        assert!(!q.observe(t0 + Duration::from_secs(10), 0));
    }
}
