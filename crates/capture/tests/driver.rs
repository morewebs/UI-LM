//! The driver against a real child process: spawn, read to EOF, read the screen.
//!
//! Every test here spawns a program on a pty rather than mocking one: what can go
//! wrong in this layer — output that never arrives, a stream that ends too early,
//! keystrokes that never reach the child — is a property of the platform's pty
//! implementation, and a mock would assert only what this file already believes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ui_lm_capture::{Driver, SessionConfig};
use ui_lm_frames::grid_from_screen;

/// How long a test that spawns a child may run before the watchdog aborts the
/// test process.
///
/// A driver that never reaches EOF must fail the test rather than hang the suite:
/// a hung capture rig and a crashed one look the same in a log, but only one of
/// them gets noticed.
const WATCHDOG_DEADLINE: Duration = Duration::from_secs(20);

/// The deadline a polling loop checks for itself, kept short of [`WATCHDOG_DEADLINE`]
/// so that an ordinary hang reports as a failed assertion carrying that test's own
/// message rather than as an abort of the whole process.
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
/// A test that can hang is a defect, not a slow test: Task 4's RED run sat for
/// nine minutes on a read that never reached EOF, and nothing in the suite said
/// so. Every test in this file that starts a real child arms this, so a
/// regression fails loudly instead of consuming a session — a detached thread
/// sleeps [`WATCHDOG_DEADLINE`], and if the test has still not returned it names
/// the test on stderr and aborts the process. That message is the whole diagnosis
/// available from CI output alone.
///
/// The returned guard disarms the watchdog when the test returns, so a test that
/// passed in a second cannot abort the harness while a later test is running. A
/// test that hangs never reaches the guard's drop, which is the point.
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
                     aborting the test process; a hang here is a driver defect, not a slow test"
                );
                std::process::abort();
            }
        })
        .expect("the watchdog thread should spawn");
    Watchdog { armed }
}

#[test]
fn captures_output_from_a_spawned_process() {
    let _watchdog = arm_watchdog("captures_output_from_a_spawned_process");
    let cfg = SessionConfig {
        command: if cfg!(windows) {
            "cmd".into()
        } else {
            "sh".into()
        },
        args: if cfg!(windows) {
            vec!["/c".into(), "echo hello".into()]
        } else {
            vec!["-c".into(), "printf hello".into()]
        },
        rows: 5,
        cols: 20,
        env: vec![("TERM".into(), "xterm-256color".into())],
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    let mut buf = [0u8; 1024];
    loop {
        match d.read_some(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
    let g = grid_from_screen(d.screen(), 20);
    assert!(g[0][0..5]
        .iter()
        .map(|c| c.ch)
        .collect::<String>()
        .contains("hello"));
    // The stream ended because the child exited, so its status is available and
    // clean: an EOF that arrived while the child was still running would mean an
    // ended session reported as a successful one.
    assert_eq!(d.try_wait().unwrap(), Some(0));
}

#[test]
fn a_quiet_child_is_not_the_end_of_the_stream() {
    let _watchdog = arm_watchdog("a_quiet_child_is_not_the_end_of_the_stream");
    // The child prints, waits, then prints again. During the wait nothing is
    // queued while the child is still alive; treating that as EOF would truncate
    // the session at "first" and lose everything a quiet moment preceded. This is
    // the failure the driver's buffered reader exists to avoid.
    let cfg = SessionConfig {
        command: if cfg!(windows) {
            "cmd".into()
        } else {
            "sh".into()
        },
        args: if cfg!(windows) {
            vec![
                "/c".into(),
                "echo first & ping -n 2 127.0.0.1 >nul & echo second".into(),
            ]
        } else {
            vec!["-c".into(), "echo first; sleep 1; echo second".into()]
        },
        rows: 5,
        cols: 20,
        env: vec![],
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    let deadline = Instant::now() + POLL_DEADLINE;
    let mut buf = [0u8; 1024];
    let mut saw_would_block = false;
    loop {
        assert!(Instant::now() < deadline, "the stream never ended");
        match d.read_some(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                saw_would_block = true;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("read failed: {e}"),
        }
    }
    assert!(
        saw_would_block,
        "the pause between the two lines should have surfaced as WouldBlock"
    );
    let g = grid_from_screen(d.screen(), 20);
    let screen: String = g
        .iter()
        .map(|row| row.iter().map(|c| c.ch).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(screen.contains("first"), "screen was {screen:?}");
    assert!(
        screen.contains("second"),
        "output after the pause was lost: {screen:?}"
    );
    assert_eq!(d.try_wait().unwrap(), Some(0));
}

#[test]
fn keystrokes_reach_the_child() {
    let _watchdog = arm_watchdog("keystrokes_reach_the_child");
    // The child turns its input into text the terminal never echoed — "got[abc]"
    // is the child's own rendering of the line it read — so the assertion cannot
    // be satisfied by the pty echoing our keystrokes back at us.
    let cfg = SessionConfig {
        command: if cfg!(windows) {
            "cmd".into()
        } else {
            "sh".into()
        },
        args: if cfg!(windows) {
            vec![
                "/v:on".into(),
                "/c".into(),
                "(set /p l= & echo got[!l!])".into(),
            ]
        } else {
            vec!["-c".into(), "read l; echo \"got[$l]\"".into()]
        },
        rows: 5,
        cols: 20,
        env: vec![],
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    let keystrokes: &[u8] = if cfg!(windows) {
        &b"abc\r\n"[..]
    } else {
        &b"abc\n"[..]
    };
    let deadline = Instant::now() + POLL_DEADLINE;
    let mut buf = [0u8; 1024];
    let mut typed = false;
    let mut answered = false;
    loop {
        assert!(
            Instant::now() < deadline,
            "the child never answered the input"
        );
        if !typed {
            d.write_input(keystrokes).unwrap();
            typed = true;
        }
        match d.read_some(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                let screen: String = (0..5)
                    .map(|row| {
                        (0..20)
                            .map(|col| {
                                d.screen()
                                    .cell(row, col)
                                    .map(|c| c.contents().chars().next().unwrap_or(' '))
                                    .unwrap_or(' ')
                            })
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if screen.contains("got[abc]") {
                    answered = true;
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("read failed: {e}"),
        }
    }
    assert!(
        answered,
        "the child never rendered the line it was sent, so the keystrokes did not reach it"
    );
}

#[test]
fn resize_resizes_the_emulated_screen() {
    let _watchdog = arm_watchdog("resize_resizes_the_emulated_screen");
    // The child is long-lived on purpose: this is about the geometry the frames
    // are read at, and a child that exits mid-resize would make the failure
    // ambiguous between a resize bug and a session that had already ended.
    let cfg = SessionConfig {
        command: if cfg!(windows) {
            "cmd".into()
        } else {
            "sh".into()
        },
        args: if cfg!(windows) {
            vec!["/c".into(), "ping -n 10 127.0.0.1 >nul".into()]
        } else {
            vec!["-c".into(), "sleep 10".into()]
        },
        rows: 5,
        cols: 20,
        env: vec![],
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    assert_eq!(d.screen().size(), (5, 20));
    d.resize(10, 30).unwrap();
    assert_eq!(d.screen().size(), (10, 30));
    let g = grid_from_screen(d.screen(), 30);
    assert_eq!(g.len(), 10);
    assert!(g.iter().all(|row| row.len() == 30));
}
