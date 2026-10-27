//! The driver against a real child process: spawn, read to EOF, read the screen.
//!
//! Every test here spawns a program on a pty rather than mocking one: what can go
//! wrong in this layer — output that never arrives, a stream that ends too early,
//! keystrokes that never reach the child — is a property of the platform's pty
//! implementation, and a mock would assert only what this file already believes.

use std::time::{Duration, Instant};

use ui_lm_capture::{Driver, SessionConfig};
use ui_lm_frames::grid_from_screen;

/// The longest any of these tests waits for a real child before failing.
///
/// A driver that never reaches EOF must fail the test rather than hang the suite:
/// a hung capture rig and a crashed one look the same in a log, but only one of
/// them gets noticed.
const DEADLINE: Duration = Duration::from_secs(20);

#[test]
fn captures_output_from_a_spawned_process() {
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
    let deadline = Instant::now() + DEADLINE;
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
    let deadline = Instant::now() + DEADLINE;
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
