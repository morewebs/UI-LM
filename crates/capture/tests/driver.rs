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
        cwd: None,
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
        cwd: None,
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
        cwd: None,
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
    // Nothing this session owed the child was refused: the write queue only
    // sheds answers when a child has stopped collecting them.
    assert_eq!(d.dropped_replies(), 0);
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
        cwd: None,
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    assert_eq!(d.screen().size(), (5, 20));
    d.resize(10, 30).unwrap();
    assert_eq!(d.screen().size(), (10, 30));
    let g = grid_from_screen(d.screen(), 30);
    assert_eq!(g.len(), 10);
    assert!(g.iter().all(|row| row.len() == 30));
}

/// How long the query-flood test keeps reading. It does not wait for EOF: on
/// Windows the flood child never exits (see that test), so the test measures a
/// fixed window of reading instead of a stream end.
const FLOOD_WINDOW: Duration = Duration::from_secs(2);

#[test]
fn a_child_that_floods_cursor_queries_without_reading_does_not_stall_the_reader() {
    let _watchdog = arm_watchdog(
        "a_child_that_floods_cursor_queries_without_reading_does_not_stall_the_reader",
    );
    // The child types a file of twenty thousand cursor queries and never reads its
    // own input, so every answer it is owed piles up uncollected. Answering those
    // inline — a write to a pty whose child is not reading — is the hang this test
    // exists to catch: `read_some` must keep returning so a record loop keeps
    // running, with the answers it cannot deliver dropped and counted instead of
    // queued without limit.
    //
    // On Windows conhost answers a client's own DSR itself and stops consuming the
    // client's output once the client is not collecting answers, so the flood does
    // not reach the reader from here (measured: a child's `A ESC [ 6 n B` arrives
    // as `AB`). The read-loop assertions still run on both platforms; the flood
    // assertions are reachable on Unix, where the child's queries are plain bytes.
    let queries = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(queries.path(), "\x1b[6n".repeat(20_000)).unwrap();
    // The path goes into the command line unquoted: `cmd /c` re-parses its
    // arguments and mangles embedded quotes. A temporary path holds no spaces on
    // the platforms this runs on; if one ever did, the child would fail loudly
    // here rather than quietly test nothing.
    let path = queries.path().display().to_string();
    let cfg = SessionConfig {
        command: if cfg!(windows) {
            "cmd".into()
        } else {
            "sh".into()
        },
        args: if cfg!(windows) {
            vec!["/c".into(), format!("type {path}")]
        } else {
            vec!["-c".into(), format!("cat '{path}'")]
        },
        rows: 5,
        cols: 20,
        env: vec![],
        cwd: None,
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    let deadline = Instant::now() + FLOOD_WINDOW;
    let mut buf = [0u8; 4096];
    let mut reads = 0usize;
    let mut bytes = 0usize;
    let mut queries_seen = 0usize;
    while Instant::now() < deadline {
        match d.read_some(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                bytes += n;
                queries_seen += buf[..n].windows(4).filter(|w| *w == b"\x1b[6n").count();
                reads += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => reads += 1,
            Err(e) => panic!("read failed: {e}"),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let dropped = d.dropped_replies();
    // Roughly 400 reads fit in the window at the sleep above. Anything near the
    // count means the read path never stopped returning: had it blocked inside a
    // reply write, the watchdog would have aborted the run instead.
    assert!(
        reads >= 50,
        "the read loop stopped observing the stream: {reads} reads in {FLOOD_WINDOW:?}"
    );
    assert!(bytes > 0, "the session produced nothing at all");
    if !cfg!(windows) {
        assert!(
            queries_seen > 1_000,
            "the flood never reached the reader: {queries_seen} queries in {bytes} bytes"
        );
        assert!(
            dropped > 0,
            "answers past the write queue must be dropped and counted, but none were: {dropped}"
        );
    }
}

/// The size of the screen the working-directory tests read: wide enough that a
/// temporary directory's path is one line rather than a wrapped fragment.
const DIRECTORY_SCREEN: (u16, u16) = (10, 200);

/// A child that prints its own working directory and exits.
///
/// `cd` and `pwd` are the platform's own answer to "where am I", so what the
/// assertion reads is the directory the child really ran in — not a value the
/// rig believes it asked for.
fn prints_its_directory() -> (String, Vec<String>) {
    if cfg!(windows) {
        ("cmd".to_owned(), vec!["/c".to_owned(), "cd".to_owned()])
    } else {
        ("sh".to_owned(), vec!["-c".to_owned(), "pwd".to_owned()])
    }
}

/// Reads until the stream ends, failing rather than hanging if it never does.
fn read_to_end(d: &mut Driver) {
    let deadline = Instant::now() + POLL_DEADLINE;
    let mut buf = [0u8; 4096];
    loop {
        assert!(Instant::now() < deadline, "the child's stream never ended");
        match d.read_some(&mut buf) {
            Ok(0) => return,
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("read failed: {e}"),
        }
    }
}

/// The screen the driver holds, as text: one line per row, padding removed.
fn screen_text(d: &Driver, rows: u16, cols: u16) -> String {
    (0..rows)
        .map(|row| {
            let line: String = (0..cols)
                .map(|col| {
                    d.screen()
                        .cell(row, col)
                        .map(|cell| cell.contents().chars().next().unwrap_or(' '))
                        .unwrap_or(' ')
                })
                .collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The directory a child that printed its own working directory reported: the
/// first line of its screen that is not blank.
fn reported_directory(screen: &str) -> &str {
    screen
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_else(|| panic!("the child printed no directory: {screen:?}"))
}

/// A directory in the form two of them can be compared in.
///
/// Windows reports a path in whatever case it likes, with either separator, and
/// `std::fs::canonicalize` would add a `\\?\` prefix the child never prints — so
/// neither a byte comparison nor the canonical form is the right test.
fn comparable(path: &str) -> String {
    let trimmed = path.trim();
    let trimmed = trimmed.strip_prefix(r"\\?\").unwrap_or(trimmed);
    if cfg!(windows) {
        trimmed.replace('/', "\\").to_lowercase()
    } else {
        trimmed.to_owned()
    }
}

/// The directory the child would fall back to if nothing set it on Windows —
/// portable-pty's `USERPROFILE`, which is the behaviour this change rules out.
fn home_directory() -> Option<String> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(|home| home.to_string_lossy().into_owned())
}

#[test]
fn a_child_runs_in_the_directory_the_config_names() {
    let _watchdog = arm_watchdog("a_child_runs_in_the_directory_the_config_names");
    // The defect this guards: a capture script started in a scratch project still
    // had its child run in the user's home directory, because the driver set no
    // directory and portable-pty's default for one it was not given is the home
    // directory. The tool screens of a session were the home directory's for the
    // same reason, and a scenario could create a file there.
    let scratch = tempfile::tempdir().unwrap();
    let wanted = scratch.path().display().to_string();
    let (command, args) = prints_its_directory();
    let (rows, cols) = DIRECTORY_SCREEN;
    let cfg = SessionConfig {
        command,
        args,
        rows,
        cols,
        env: vec![],
        cwd: Some(scratch.path().to_path_buf()),
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    read_to_end(&mut d);
    let screen = screen_text(&d, rows, cols);
    let reported = reported_directory(&screen);
    assert_eq!(
        comparable(reported),
        comparable(&wanted),
        "the child did not run in the directory the config named: it reported {reported:?} \
         rather than {wanted:?}"
    );
    assert_eq!(d.try_wait().unwrap(), Some(0));
}

#[test]
fn a_child_runs_in_the_recorders_directory_when_none_is_configured() {
    let _watchdog = arm_watchdog("a_child_runs_in_the_recorders_directory_when_none_is_configured");
    // With no directory configured the child must run where the recorder itself
    // is, which is what a capture started inside a scratch project means. The
    // alternative is portable-pty's home-directory default, and that is the one
    // this test exists to rule out.
    let here = std::env::current_dir()
        .expect("the test process has a current directory")
        .display()
        .to_string();
    let home = home_directory();
    // A test process started in the home directory cannot tell "the recorder's own
    // directory" from "portable-pty's fallback": both name the same path, and the
    // assertion would hold for the wrong reason. Say so rather than pass vacuously.
    if home
        .as_deref()
        .is_some_and(|home| comparable(home) == comparable(&here))
    {
        eprintln!(
            "skipped: this test process's own directory {here:?} is the home directory, so a \
             child that fell back to home would report exactly the path this test expects"
        );
        return;
    }
    let (command, args) = prints_its_directory();
    let (rows, cols) = DIRECTORY_SCREEN;
    let cfg = SessionConfig {
        command,
        args,
        rows,
        cols,
        env: vec![],
        cwd: None,
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    read_to_end(&mut d);
    let screen = screen_text(&d, rows, cols);
    let reported = reported_directory(&screen);
    assert_eq!(
        comparable(reported),
        comparable(&here),
        "a child with no configured directory must run in the recorder's own directory, not in \
         portable-pty's home-directory default: it reported {reported:?}, and the recorder is in \
         {here:?}"
    );
    if let Some(home) = home.as_deref() {
        assert_ne!(
            comparable(reported),
            comparable(home),
            "the child reported the home directory {home:?}, which is the fallback this test \
             exists to rule out"
        );
    }
    assert_eq!(d.try_wait().unwrap(), Some(0));
}
