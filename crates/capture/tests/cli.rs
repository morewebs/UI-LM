//! The capture CLI end to end: the real binary, a real child on a pty, a session
//! on disk.
//!
//! Nothing here is mocked. The point of this file is that the whole rig composes —
//! the driver's reader thread, the emulator, the quiescer, the mask and the session
//! writer — and a test that stubbed any one of them would assert only that the
//! stub was told what the test already believed. These tests run the binary the
//! way a capture script does, then read the session back the way the validator
//! will.

use std::path::Path;
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ui_lm_frames::{decode_grid, read_frames, Frame};

/// How long a test that spawns a child may run before the watchdog aborts the
/// test process.
///
/// A recorder that never reaches the end of its stream must fail the test rather
/// than hang the suite: a hung capture rig and a crashed one look the same in a
/// log, but only one of them gets noticed.
const WATCHDOG_DEADLINE: Duration = Duration::from_secs(20);

/// The deadline a polling loop checks for itself, kept short of
/// [`WATCHDOG_DEADLINE`] so that an ordinary hang reports as a failed assertion
/// carrying that test's own message rather than as an abort of the whole process.
const POLL_DEADLINE: Duration = Duration::from_secs(15);

/// An armed watchdog; disarms it when dropped.
struct Watchdog {
    armed: Arc<AtomicBool>,
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.armed.store(false, Ordering::SeqCst);
    }
}

/// Arms the watchdog for one test that spawns a child process.
///
/// Every test in this file starts a real program on a pty, so every test in this
/// file arms this. A detached thread sleeps [`WATCHDOG_DEADLINE`], and if the test
/// has still not returned it names the test on stderr and aborts the process: that
/// message is the whole diagnosis available from CI output alone.
fn arm_watchdog(test_name: &str) -> Watchdog {
    let armed = Arc::new(AtomicBool::new(true));
    let armed_while_hanging = Arc::clone(&armed);
    let name = test_name.to_owned();
    std::thread::Builder::new()
        .name(format!("watchdog: {test_name}"))
        .spawn(move || {
            std::thread::sleep(WATCHDOG_DEADLINE);
            if armed_while_hanging.load(Ordering::SeqCst) {
                eprintln!(
                    "watchdog: test `{name}` exceeded its {WATCHDOG_DEADLINE:?} deadline and is \
                     aborting the test process; a hang here is a recorder defect, not a slow test"
                );
                std::process::abort();
            }
        })
        .expect("the watchdog thread should spawn");
    Watchdog { armed }
}

/// The recorder, pointed at `out` on a 10x40 screen.
fn recorder(out: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ui-lm-capture"));
    command
        .args(["record", "--rows", "10", "--cols", "40", "--out"])
        .arg(out);
    command
}

/// The fixture for the end-to-end test: three lines, each separated by a pause
/// longer than the rig's 150 ms settle gap.
///
/// The pause is the whole point. Without one, all three lines arrive inside a
/// single burst, the screen settles once, and there is one frame with nothing to
/// diff — which is correct behaviour and a useless test. The pause is what makes
/// each line a settled screen of its own.
///
/// `ping` rather than `timeout` for the sleep on Windows: `timeout` refuses to run
/// with its input redirected, which is exactly the situation on a pty.
fn three_paused_lines() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd".into(),
            "/c".into(),
            "echo one & ping -n 2 127.0.0.1 -w 400 >nul & echo two & ping -n 2 127.0.0.1 -w 400 \
             >nul & echo three"
                .into(),
        ]
    } else {
        vec![
            "sh".into(),
            "-c".into(),
            "echo one; sleep 0.4; echo two; sleep 0.4; echo three".into(),
        ]
    }
}

/// A child that reads one line of its input and prints what it read.
///
/// The child renders the line itself — `got[abc]` is its own text, not the echo
/// of what was typed — so a frame containing it cannot have come from the pty
/// echoing keystrokes back at the recorder.
fn reads_a_line() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd".into(),
            "/v:on".into(),
            "/c".into(),
            "(set /p l= & echo got[!l!])".into(),
        ]
    } else {
        vec!["sh".into(), "-c".into(), "read l; echo \"got[$l]\"".into()]
    }
}

/// The keystroke spec, as written into a `--script` file, that answers
/// [`reads_a_line`].
///
/// The escapes are the script file's, not Rust's: `\r` is the two characters that
/// the script reader turns back into a carriage return.
fn line_keystroke() -> &'static str {
    if cfg!(windows) {
        r"abc\r"
    } else {
        r"abc\n"
    }
}

/// All the characters a frame holds, one line per row.
fn frame_text(frame: &Frame) -> String {
    decode_grid(&frame.runs, frame.rows, frame.cols)
        .iter()
        .map(|row| row.iter().map(|cell| cell.ch).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn record_writes_frames_changed_rows_and_session() {
    let _watchdog = arm_watchdog("record_writes_frames_changed_rows_and_session");
    let out = tempfile::tempdir().unwrap();
    let status = recorder(out.path())
        .arg("--")
        .args(three_paused_lines())
        .status()
        .unwrap();
    assert!(status.success(), "the recorder failed: {status}");

    let (record, frames) = read_frames(out.path()).unwrap();
    assert!(
        frames.len() >= 3,
        "expected one frame per settled line, got {}",
        frames.len()
    );
    // The first frame is the whole first screen: there is no screen before it.
    assert_eq!(frames[0].changed_rows, (0u16..10).collect::<Vec<_>>());
    // The second frame is the first line plus the second, so fewer rows than a
    // whole screen moved — and at least one of them did.
    assert!(!frames[1].changed_rows.is_empty());
    assert!(frames[1].changed_rows.len() < 10);
    assert_eq!(frames[0].seq, 0);
    assert_eq!(frames[1].seq, 1);
    assert!(record.cost.is_none());
    assert_eq!(record.rows, 10);
    assert_eq!(record.cols, 40);
    assert!(record.ended_at.is_some(), "a finished capture says when");
}

/// The fixture for the failing-child test: draws a line, then exits non-zero.
///
/// The line matters as much as the code: it is what makes the session a real one,
/// so the test can show that a written session and a failing program are two
/// different facts rather than one.
fn fails_after_drawing() -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd".into(), "/c".into(), "echo done & exit 3".into()]
    } else {
        vec!["sh".into(), "-c".into(), "echo done; exit 3".into()]
    }
}

#[test]
fn a_non_zero_child_exit_is_reported_but_not_propagated() {
    let _watchdog = arm_watchdog("a_non_zero_child_exit_is_reported_but_not_propagated");
    let out = tempfile::tempdir().unwrap();
    let output = recorder(out.path())
        .arg("--")
        .args(fails_after_drawing())
        .output()
        .unwrap();

    // The recorder's exit code describes the recorder's job, and its job is to
    // write the session — which it did. A scenario that deliberately kills the
    // application must not read as a failed capture to the script running it.
    assert!(
        output.status.success(),
        "a written session is a successful capture, however the program ended; the \
         recorder exited {}",
        output.status
    );

    // The child's failure still has to be visible for triage: on stderr, where it
    // cannot be mistaken for the recorder's own verdict, and naming the code.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("exited with status 3")),
        "the child's exit status must still be reported on stderr: {stderr:?}"
    );

    // And the session is the real thing: a record, and the frames the child drew
    // before it stopped. A capture failure is distinguishable from a failing
    // program precisely because it leaves no `session.json`, not by the code.
    let (record, frames) =
        read_frames(out.path()).expect("a capture of a failing program still writes its record");
    assert!(
        !frames.is_empty(),
        "the child drew a line before it exited, so the session has the frame it drew"
    );
    assert!(record.ended_at.is_some(), "the record says how it ended");
}

#[test]
fn a_scripted_keystroke_is_written_through_to_the_child() {
    let _watchdog = arm_watchdog("a_scripted_keystroke_is_written_through_to_the_child");
    let out = tempfile::tempdir().unwrap();
    let script = tempfile::NamedTempFile::new().unwrap();
    // A 200 ms wait before the keystroke: the delay is the script's way of letting
    // the application finish starting up before it is typed at.
    std::fs::write(script.path(), format!("200 {}\n", line_keystroke())).unwrap();

    let status = recorder(out.path())
        .arg("--script")
        .arg(script.path())
        .arg("--")
        .args(reads_a_line())
        .status()
        .unwrap();
    assert!(status.success(), "the recorder failed: {status}");

    let (_, frames) = read_frames(out.path()).unwrap();
    assert!(
        !frames.is_empty(),
        "a child that drew something must produce frames"
    );
    let screen = frame_text(frames.last().unwrap());
    assert!(
        screen.contains("got[abc]"),
        "the scripted keystroke never reached the child, so the session is not the \
         one the script asked for: {screen:?}"
    );
}

/// The fixture for the interrupt test: draws a line, then stays alive.
///
/// It has to still be running when the interrupt arrives, and it has to have
/// drawn something settled before it — an interrupt before the first frame would
/// only prove that an empty session is still a valid one.
fn draws_then_waits() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd".into(),
            "/c".into(),
            "echo running & ping -n 30 127.0.0.1 >nul".into(),
        ]
    } else {
        vec!["sh".into(), "-c".into(), "echo running; sleep 30".into()]
    }
}

#[cfg(windows)]
mod interrupt {
    use super::Child;

    /// The console event that stands in for Ctrl-C here.
    const CTRL_BREAK_EVENT: u32 = 1;
    /// The creation flag that puts the recorder in a process group of its own.
    pub const NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    extern "system" {
        fn GenerateConsoleCtrlEvent(event: u32, process_group: u32) -> i32;
    }

    /// Sends the recorder's process group a break.
    ///
    /// Windows cannot deliver a `CTRL_C_EVENT` to one process: that event is only
    /// accepted with a process group of zero, which would signal the test harness
    /// and cargo as well. `CTRL_BREAK_EVENT` *can* be aimed at a process group, the
    /// recorder is started in one of its own, and the recorder handles both events
    /// through one handler — so this exercises the same code path as a user
    /// pressing Ctrl-C.
    pub fn send_interrupt(child: &Child) {
        let sent = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) };
        assert_ne!(
            sent,
            0,
            "GenerateConsoleCtrlEvent failed: {}",
            std::io::Error::last_os_error()
        );
    }
}

#[cfg(unix)]
mod interrupt {
    use super::Child;

    /// Sends the recorder a `SIGINT`, which is what Ctrl-C delivers on a Unix
    /// terminal.
    pub fn send_interrupt(child: &Child) {
        let status = std::process::Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .expect("kill should run");
        assert!(status.success(), "kill -INT failed");
    }
}

/// How the recorder is started for the interrupt test: in a process group of its
/// own, so the interrupt can be aimed at it alone.
fn spawn_in_own_group(command: &mut Command) -> Child {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(interrupt::NEW_PROCESS_GROUP);
    }
    command.spawn().expect("the recorder should spawn")
}

/// Waits until at least `wanted` frames are on disk.
fn wait_for_frames(dir: &Path, wanted: usize) {
    let deadline = Instant::now() + POLL_DEADLINE;
    loop {
        let lines = std::fs::read_to_string(dir.join("frames.jsonl"))
            .map(|text| text.lines().count())
            .unwrap_or(0);
        if lines >= wanted {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "only {lines} frame(s) reached the session directory within {POLL_DEADLINE:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits for the recorder to exit, rather than hanging the suite on it.
fn wait_for_exit(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + POLL_DEADLINE;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "the recorder did not exit within {POLL_DEADLINE:?} of being interrupted"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn an_interrupt_finishes_the_session_instead_of_leaving_it_unfinished() {
    let _watchdog =
        arm_watchdog("an_interrupt_finishes_the_session_instead_of_leaving_it_unfinished");
    let out = tempfile::tempdir().unwrap();
    let mut command = recorder(out.path());
    command.arg("--").args(draws_then_waits());
    let mut child = spawn_in_own_group(&mut command);

    // The interrupt has to land mid-session to mean anything: a capture stopped
    // before its first frame would only prove that an empty session is valid.
    wait_for_frames(out.path(), 1);
    interrupt::send_interrupt(&child);
    let status = wait_for_exit(&mut child);

    // A clean interrupt is a finished capture, not a failed one: the recorder
    // writes its record and exits successfully, so a capture script can tell this
    // apart from a crash. Only a kill leaves frames with no record, which is what
    // `read_orphaned_frames` is for.
    assert!(status.success(), "an interrupted capture exited {status}");
    let (record, frames) = read_frames(out.path()).expect(
        "an interrupted capture must still be a finished session with a record; only a \
         hard kill leaves orphaned frames",
    );
    assert!(
        !frames.is_empty(),
        "the frames written before the interrupt are the session"
    );
    assert!(record.ended_at.is_some(), "the record says how it ended");
}
