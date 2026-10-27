//! The validator CLI end to end: a session the recorder wrote, a dump file the fork
//! would have written, and the real binary's exit code.
//!
//! What is under test here is the contract a capture script depends on — a clean
//! comparison exits 0, a disagreement exits non-zero and says where — so the test
//! runs the binary rather than calling into it. A session is written with the same
//! `SessionWriter` the recorder uses, so the fixture is the real on-disk format
//! rather than a hand-rolled approximation of it.

use std::path::Path;
use std::process::{Command, Output};

use ui_lm_frames::{frame_from_chars, Frame, SessionRecord, SessionWriter};

/// One dumped cell, in the shape the patched fork writes: a symbol in the default
/// colour, with none of the four modifier bits set.
fn cell(row: u16, col: u16, ch: char) -> String {
    format!(
        r#"{{"row":{row},"col":{col},"symbol":"{ch}","fg":"reset","bg":"reset","bold":false,"italic":false,"underline":false,"reverse":false}}"#
    )
}

/// One dump line: a 1x2 screen holding `left` and `right`.
fn dump_line(seq: u32, left: char, right: char) -> String {
    format!(
        r#"{{"seq":{seq},"rows":1,"cols":2,"cells":[{},{}]}}"#,
        cell(0, 0, left),
        cell(0, 1, right)
    )
}

/// Writes a finished session of 1x2 frames into `dir`, the way the recorder does.
fn write_session(dir: &Path, screens: &[[char; 2]]) {
    let record = SessionRecord {
        version: 1,
        started_at: "2026-10-27T10:00:00Z".to_owned(),
        ended_at: None,
        command: "grok".to_owned(),
        args: Vec::new(),
        rows: 1,
        cols: 2,
        env: vec![("TERM".to_owned(), "xterm-256color".to_owned())],
        app_version: None,
        cost: None,
    };
    let mut writer = SessionWriter::new(dir, record).expect("the session opens");
    for (seq, screen) in screens.iter().enumerate() {
        writer
            .append(&frame_from_chars(seq as u32, 1, 2, &[screen.to_vec()]))
            .expect("the frame appends");
    }
    writer.finish(None).expect("the session finishes");
}

/// Writes `lines` as a dump file and returns its path.
fn write_dump(dir: &Path, lines: &[String]) -> std::path::PathBuf {
    let path = dir.join("buffer.jsonl");
    std::fs::write(&path, lines.join("\n") + "\n").expect("the dump is written");
    path
}

/// Runs `ui-lm-validate compare` with the given arguments.
fn compare(ours: &Path, theirs: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ui-lm-validate"))
        .arg("compare")
        .arg("--ours")
        .arg(ours)
        .arg("--theirs")
        .arg(theirs)
        .args(extra)
        .output()
        .expect("the validator runs")
}

/// A binary's stdout, as text.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_matching_final_screen_exits_zero() {
    // The default comparison: our last frame against the fork's last draw. This is
    // the pair K5 reads its verdict from, so a clean pair has to be a success exit
    // with the counts reported.
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session");
    write_session(&session, &[['a', 'b'], ['c', 'd']]);
    let dump = write_dump(
        dir.path(),
        &[dump_line(0, 'a', 'b'), dump_line(1, 'c', 'd')],
    );

    let output = compare(&session, &dump, &[]);
    let text = stdout(&output);
    assert!(
        output.status.success(),
        "identical final screens must exit 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("total cells: 2") && text.contains("mismatched:  0"),
        "the counts must be reported: {text}"
    );
}

#[test]
fn a_differing_cell_exits_non_zero_and_names_its_position() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session");
    write_session(&session, &[['a', 'b'], ['c', 'd']]);
    let dump = write_dump(
        dir.path(),
        &[dump_line(0, 'a', 'b'), dump_line(1, 'c', 'X')],
    );

    let output = compare(&session, &dump, &[]);
    let text = stdout(&output);
    assert!(
        !output.status.success(),
        "a disagreement must exit non-zero, or a script cannot gate on it: {text}"
    );
    assert!(
        text.contains("mismatched:  1"),
        "the mismatch count must be reported: {text}"
    );
    assert!(
        text.contains("first mismatch: 0,1: ours='d' theirs='X'"),
        "the first mismatch must be reported as row,col with both characters: {text}"
    );
}

#[test]
fn a_named_seq_compares_that_screen_rather_than_the_last_one() {
    // The end-of-session pair is the default, not the only option: `--ours-seq` and
    // `--theirs-seq` name a screen by its own number, and the two sides are numbered
    // by their own writers.
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session");
    write_session(&session, &[['a', 'b'], ['c', 'd']]);
    // Draw 0 agrees with frame 0; the last draw disagrees with the last frame. A
    // comparison that ignored the flags would fail here.
    let dump = write_dump(
        dir.path(),
        &[dump_line(0, 'a', 'b'), dump_line(1, 'c', 'X')],
    );

    let output = compare(&session, &dump, &["--ours-seq", "0", "--theirs-seq", "0"]);
    let text = stdout(&output);
    assert!(
        output.status.success(),
        "frames 0 and 0 hold the same screen, so they must compare clean: {text}"
    );
}

#[test]
fn a_seq_that_does_not_exist_is_an_error_rather_than_the_default() {
    // Silently falling back to the last screen would turn a typo into a comparison
    // nobody asked for, and its verdict would be reported as K5's.
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session");
    write_session(&session, &[['a', 'b']]);
    let dump = write_dump(dir.path(), &[dump_line(0, 'a', 'b')]);

    let output = compare(&session, &dump, &["--ours-seq", "7"]);
    assert!(
        !output.status.success(),
        "an unknown seq is not a clean run"
    );
    let message = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        message.contains('7'),
        "the error must name the seq that was asked for: {message}"
    );
}

/// A frame nobody rewrote still reads back as the frame that was written: the
/// fixture's own sanity check, so a failing comparison test is about the comparison
/// rather than about the session format.
#[test]
fn the_session_fixture_reads_back_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session");
    write_session(&session, &[['a', 'b'], ['c', 'd']]);
    let (_, frames): (SessionRecord, Vec<Frame>) =
        ui_lm_frames::read_frames(&session).expect("the session reads");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[1].runs[0].ch, 'c');
}
