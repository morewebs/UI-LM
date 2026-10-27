//! The capture CLI end to end: the real binary, a real child on a pty, a session
//! on disk.
//!
//! Nothing here is mocked. The point of this file is that the whole rig composes —
//! the driver's reader thread, the emulator, the quiescer, the mask and the session
//! writer — and a test that stubbed any one of them would assert only that the
//! stub was told what the test already believed. These tests run the binary the
//! way a capture script does, then read the session back the way the validator
//! will.

mod common;

use common::comparable;

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

/// A child that prints its own working directory and exits.
///
/// `cd` and `pwd` are the platform's own answer to "where am I", so a frame
/// holding one of their answers holds the directory the child really ran in.
fn prints_its_directory() -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd".into(), "/c".into(), "cd".into()]
    } else {
        vec!["sh".into(), "-c".into(), "pwd".into()]
    }
}

/// The directory a child that printed its own working directory reported: the
/// first line of its screen that is not blank.
fn reported_directory(screen: &str) -> String {
    screen
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_else(|| panic!("the child printed no directory: {screen:?}"))
        .to_owned()
}

#[test]
fn the_cwd_flag_puts_the_child_in_the_directory_it_names() {
    let _watchdog = arm_watchdog("the_cwd_flag_puts_the_child_in_the_directory_it_names");
    // Two scratch directories: the recorder is started in one and told about the
    // other. Only a `--cwd` that reaches the driver — through the argument
    // parser, the session config and the command builder — can put the child in
    // the second, so this cannot pass because the recorder happened to inherit a
    // directory.
    let recorder_dir = tempfile::tempdir().unwrap();
    let child_dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    // Wider than [`recorder`]'s screen: a temporary directory's path is longer
    // than forty columns, and a truncated path is not a directory the assertion
    // could compare.
    let output = Command::new(env!("CARGO_BIN_EXE_ui-lm-capture"))
        .args(["record", "--rows", "10", "--cols", "200", "--out"])
        .arg(out.path())
        .arg("--cwd")
        .arg(child_dir.path())
        .current_dir(recorder_dir.path())
        .arg("--")
        .args(prints_its_directory())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the recorder failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let (_, frames) = read_frames(out.path()).unwrap();
    let screen = frame_text(
        frames
            .last()
            .expect("a child that printed a directory drew a frame"),
    );
    let reported = reported_directory(&screen);
    assert_eq!(
        comparable(&reported),
        comparable(&child_dir.path().display().to_string()),
        "the child did not run in the directory `--cwd` named: it reported {reported:?} rather \
         than {:?}",
        child_dir.path().display().to_string()
    );
    assert_ne!(
        comparable(&reported),
        comparable(&recorder_dir.path().display().to_string()),
        "the child ran in the recorder's own directory, so `--cwd` never reached the driver: it \
         reported {reported:?}"
    );
}

/// A child that draws on the primary screen, switches to the alternate screen, and
/// draws there.
///
/// The switch is the application's own escape sequence, `ESC [ ? 1049 h`, written by
/// the child rather than arranged by the test: what is being checked is what reaches
/// the recorder from a real pty, not what the emulator does with a sequence the test
/// handed it (`ui_lm_capture::screen`'s tests cover that half on the real parser).
///
/// `ping` rather than `timeout` for the pause on Windows: `timeout` refuses to run
/// with its input redirected, which is exactly the situation on a pty. The pause
/// before the switch is what makes the first screen a settled frame of its own.
fn switches_to_the_alternate_screen() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "cmd".into(),
            "/c".into(),
            "echo primary-screen & ping -n 2 127.0.0.1 -w 400 >nul & echo \x1b[?1049h & ping -n \
             2 127.0.0.1 -w 400 >nul & echo alt-surface"
                .into(),
        ]
    } else {
        vec![
            "sh".into(),
            "-c".into(),
            "printf 'primary-screen\\n'; sleep 0.4; printf '\\033[?1049h'; printf \
             'alt-surface'; sleep 0.4"
                .into(),
        ]
    }
}

#[test]
fn a_child_that_switches_to_the_alternate_screen_is_recorded() {
    let _watchdog = arm_watchdog("a_child_that_switches_to_the_alternate_screen_is_recorded");
    let out = tempfile::tempdir().unwrap();
    let output = recorder(out.path())
        .arg("--")
        .args(switches_to_the_alternate_screen())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the recorder failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let (_, frames) = read_frames(out.path()).unwrap();
    let last = frames
        .last()
        .expect("a child that drew two screens produced frames");
    let screen = frame_text(last);

    // What both pty hosts agree on: the switch replaced the surface. The children
    // above draw `primary-screen`, then switch, then draw `alt-surface`; the last
    // frame holds the second and not the first, so the screen it describes is the
    // one the child drew after the switch.
    assert!(
        screen.contains("alt-surface"),
        "the frame after the alternate-screen switch must hold what the child drew on it: \
         {screen:?}"
    );
    assert!(
        !screen.contains("primary-screen"),
        "the alternate screen is a different surface, so the primary screen's content is not \
         on it: {screen:?}"
    );
    assert!(
        !screen.contains("1049"),
        "the switch was consumed rather than drawn as text, so the escape reached a terminal \
         and not the grid: {screen:?}"
    );

    // What the two platforms disagree about. A Unix pty hands the child's bytes over
    // unchanged, so the recorder's emulator sees the switch and the frame says so.
    //
    // `cfg!` rather than `#[cfg]` so that this half is still type-checked when the
    // suite is built on Windows, which is the platform this rig is developed on: the
    // expectation that cannot hold here should not be the expectation nothing
    // compiles.
    if cfg!(unix) {
        assert!(
            last.alt_screen,
            "the child switched to the alternate screen, so the recorded frame must say so: \
             {screen:?}"
        );
        assert_eq!(
            last.changed_rows,
            (0..last.rows).collect::<Vec<_>>(),
            "entering the alternate screen replaces the whole surface, so every row is new \
             content"
        );
    }

    // On Windows the pty host is ConPTY, and whether it forwards the switch is the
    // host's decision rather than the emulator's. Both outcomes have now been
    // observed: a development machine's conhost consumes `ESC [ ? 1049 h` and
    // re-renders the delta (the bytes a child's switch produces downstream are
    // `ESC [ 25 l ESC [ H ESC [ K CRLF …`, never the switch), while a
    // `windows-latest` runner forwarded it and the frame recorded `alt_screen:
    // true`. `portable-pty` 0.9 knows the flag that would decide this
    // (`PSEUDOCONSOLE_PASSTHROUGH_MODE`) and does not use it.
    //
    // So there is no assertion here: what the frame says is what the host did, and
    // the checks above already prove the switch reached a terminal rather than
    // being drawn as text. The Unix arm above is where the plumbing itself is
    // pinned.
    let _host_decided_the_alternate_screen = last.alt_screen;
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
    //
    // The *shape* is what this asserts, not the number. A console child can be terminated
    // by a console control event instead of exiting on its own — a `cmd /c "… & exit 3"`
    // reports `0xC000013A` on a loaded machine — and the recorder's job is to report
    // whatever status the system gave it. Asserting `3` exactly made this test fail about
    // one run in six under load, which is a test asserting a property of the platform
    // rather than of the recorder. The number's own formatting is covered by unit tests.
    assert!(
        stderr.lines().any(|line| line.contains("warning:")
            && (line.contains("exited with status") || line.contains("control event"))
            && line.contains("cmd")
            || line.contains("sh")),
        "the child's exit must still be reported on stderr: {stderr:?}"
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

/// The fixture for the delay-cap test: draws a line as soon as it starts.
///
/// It exists so that "nothing was recorded" is evidence rather than a coincidence:
/// if the recorder ran this program, a frame would be on disk, so the absence of
/// one says the script was refused before the program was ever spawned.
fn draws_a_line_at_once() -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd".into(), "/c".into(), "echo ran".into()]
    } else {
        vec!["sh".into(), "-c".into(), "echo ran".into()]
    }
}

#[test]
fn a_script_delay_over_the_cap_is_refused_naming_the_file_and_line() {
    let _watchdog = arm_watchdog("a_script_delay_over_the_cap_is_refused_naming_the_file_and_line");
    let out = tempfile::tempdir().unwrap();
    let script = tempfile::NamedTempFile::new().unwrap();
    // The first line's delay is fine; the second is 27 hours written as if it were
    // milliseconds, which is the typo this rule is for. A delay that large is not a
    // pause a script could mean, and it is large enough that adding it to the clock
    // overflowed and aborted the capture mid-session — so it must be a parse error
    // instead, naming the line it is on, before anything is spawned or written.
    std::fs::write(script.path(), "1 first\\r\n99999999 hello\\r\n").unwrap();

    let output = recorder(out.path())
        .arg("--script")
        .arg(script.path())
        .arg("--")
        .args(draws_a_line_at_once())
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "a delay over the cap is a bad script rather than a long wait, so the recorder \
         must fail; it exited {}",
        output.status
    );

    // "Clear" is both halves of where: which file, and which line of it.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let name = script
        .path()
        .file_name()
        .expect("the temp script has a name")
        .to_string_lossy()
        .into_owned();
    assert!(
        stderr.contains(&name),
        "the error must name the keystroke script it came from: {stderr:?}"
    );
    assert!(
        stderr.contains(":2:"),
        "the error must name the line the delay is on, not just the file: {stderr:?}"
    );
    assert!(
        stderr.contains("60000"),
        "the error must say what the largest delay a line may ask for is: {stderr:?}"
    );

    // And no session at all: not a record, and not an empty stream that a reader
    // could mistake for a capture that ran and drew nothing.
    assert!(
        read_frames(out.path()).is_err(),
        "a refused script must not leave a session `read_frames` would accept"
    );
    assert!(
        !out.path().join("session.json").exists(),
        "a refused script must not leave a session record"
    );
    assert!(
        !out.path().join("frames.jsonl").exists(),
        "a refused script must not leave frames behind"
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

#[test]
fn a_timed_stop_ends_the_session_and_writes_its_record() {
    // The reason this exists: every capture before it ended when the program exited
    // or when the recorder was killed, and a killed recorder leaves frames with no
    // session.json. A timed stop is a normal ending, so the session is complete.
    let _watchdog = arm_watchdog("a_timed_stop_ends_the_session_and_writes_its_record");
    let out = tempfile::tempdir().unwrap();
    let status = recorder(out.path())
        .args(["--stop-after", "1200", "--"])
        .args(draws_then_waits())
        .status()
        .unwrap();
    assert!(status.success(), "the recorder failed: {status}");

    let (record, frames) =
        read_frames(out.path()).expect("a timed stop must leave a finished session");
    assert!(!frames.is_empty(), "the child drew before the stop");
    assert!(
        record.ended_at.is_some(),
        "a finished session records when it ended: {record:?}"
    );
}
#[test]
fn render_writes_an_html_file_from_a_recorded_session() {
    // The whole loop in one command, against the checked-in recording rather than a
    // capture taken during the test: record → page → draw. If this passes, a person can
    // go from a session on disk to a file they can open in a browser.
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../pages/tests/fixtures/session");
    let out = tempfile::tempdir().unwrap();
    let page = out.path().join("page.html");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_ui-lm-capture"))
        .args(["render", "--session"])
        .arg(&fixture)
        .arg("--out")
        .arg(&page)
        .status()
        .unwrap();
    assert!(status.success(), "the renderer failed: {status}");

    let html = std::fs::read_to_string(&page).expect("the renderer must write the file");
    assert!(
        html.starts_with("<!doctype html>"),
        "not a document: {html:.80}"
    );
    assert!(
        html.contains("Grok Build fixture"),
        "the screen text is missing"
    );
    assert!(html.contains("class=\"meter\""), "the meter is missing");
    assert!(html.contains("class=\"tool\""), "the tool line is missing");
}
/// A program that says it is waiting, then waits for input.
fn waits_for_input() -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd".into(), "/c".into(), "pause".into()]
    } else {
        vec![
            "sh".into(),
            "-c".into(),
            "echo waiting for input; read line; echo got \"$line\"".into(),
        ]
    }
}

#[test]
fn a_control_file_types_into_a_running_program() {
    // The interaction primitive: something outside the recorder types, and the running
    // program receives it. The program here waits for a key and exits when one arrives,
    // so the session ending early is the evidence that the bytes landed.
    let _watchdog = arm_watchdog("a_control_file_types_into_a_running_program");
    let out = tempfile::tempdir().unwrap();
    let channel = tempfile::tempdir().unwrap();
    let keys = channel.path().join("keys.txt");

    let mut child = recorder(out.path())
        .args(["--stop-after", "15000", "--control"])
        .arg(&keys)
        .arg("--")
        .args(waits_for_input())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert!(
        child.try_wait().unwrap().is_none(),
        "the program must still be waiting when the key is typed"
    );
    std::fs::write(&keys, b"x\n").unwrap();

    let status = child.wait().unwrap();
    assert!(status.success(), "the recorder failed: {status}");

    let (_, frames) = read_frames(out.path()).expect("the session must be finished");
    assert!(
        !frames.is_empty(),
        "the waiting screen must have been recorded"
    );
}
#[test]
fn a_recording_becomes_training_pairs() {
    // The bridge from the rig to a model: each screen's text, and the page it describes.
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../pages/tests/fixtures/session");
    let out = tempfile::tempdir().unwrap();
    let pairs = out.path().join("pairs.jsonl");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_ui-lm-capture"))
        .args(["dataset", "--session"])
        .arg(&fixture)
        .arg("--out")
        .arg(&pairs)
        .status()
        .unwrap();
    assert!(status.success(), "the export failed: {status}");

    let body = std::fs::read_to_string(&pairs).expect("the export must write the file");
    let lines: Vec<&str> = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "the fixture holds one screen with content: {body}"
    );
    let pair: serde_json::Value = serde_json::from_str(lines[0]).expect("each line must be JSON");
    let input = pair["input"].as_str().unwrap_or_default();
    let target = pair["target"].as_str().unwrap_or_default();
    assert!(
        input.contains("Grok Build fixture"),
        "the screen text is the input: {input}"
    );
    assert!(
        target.contains("createSurface"),
        "the page is the target: {target}"
    );
    assert!(
        !body.contains("\n\n"),
        "one pair per line is the whole point"
    );
}
