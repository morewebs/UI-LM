//! The session on disk: one record describing the capture, one frame per line.
//!
//! A session is a directory holding two files. `session.json` is the record — what
//! was run, on what screen, and what it cost. `frames.jsonl` is the stream: one
//! `Frame` per line, appended as each screen settles. They are separate files
//! because they have different lifetimes: frames arrive for minutes, while the
//! record is only complete once the capture ends, and a reader that wants the
//! record should not have to walk a hundred megabytes of frames to find it.
//!
//! The format is a contract with everything downstream. The validator compares
//! these frames against the application's own draw buffer and the training set is
//! built from them, so a change here changes what every recorded session means.
//!
//! A capture killed before it ends has frames but no record, so the two files are
//! read by two functions: `read_frames` for a finished session, and
//! `read_orphaned_frames` for the stream a kill left behind. A directory describes
//! one capture, and `SessionWriter::new` clears the previous run's record rather
//! than letting two captures share a directory.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::frame::Frame;

/// The record: the facts about a capture, written once when it finishes.
const SESSION_FILE: &str = "session.json";
/// The frames: one JSON object per line, appended as they arrive.
const FRAMES_FILE: &str = "frames.jsonl";

/// What a finished session was, in the terms someone reading it a year later
/// would ask for.
///
/// The record is written by `finish` and never before, so a session directory
/// with a record in it is a session that ended — as opposed to one that is still
/// running, or one that was killed before it could say how it ended. That is worth
/// having as a fact on disk: a truncated frame stream is the normal cost of a
/// Ctrl-C, but a truncated *record* would be the capture losing the one thing it
/// knew from the start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    /// The session format's version.
    ///
    /// Session files outlive the binary that wrote them, so a reader has to be
    /// able to tell a format it understands from one it does not. Version 1 is
    /// the shape described by this file.
    pub version: u32,
    /// When capture began, as an RFC 3339 UTC timestamp.
    pub started_at: String,
    /// When capture ended, or `None` for a session that has not finished.
    pub ended_at: Option<String>,
    /// The program that was run, without its arguments.
    pub command: String,
    /// The arguments it was run with, in order.
    pub args: Vec<String>,
    /// Screen height the capture used, in rows.
    pub rows: u16,
    /// Screen width the capture used, in columns.
    pub cols: u16,
    /// The environment the program was run with.
    ///
    /// Pairs rather than a map: the order is the order a process environment has,
    /// a variable may be set twice, and sorting it to make it a map would hide
    /// which of the two the program saw.
    pub env: Vec<(String, String)>,
    /// The application's own version string, when it reports one.
    ///
    /// Recorded because the screens from two versions of the same program are
    /// different screens, and a training set that cannot tell them apart cannot
    /// say which one it was taught.
    pub app_version: Option<String>,
    /// What the session cost to produce, when anything priced it.
    ///
    /// Absent for a session with no model behind it, which is why it is an
    /// `Option` on the wire as a `null` rather than an optional field: every
    /// reader looks up `cost` and finds out that there was no cost.
    pub cost: Option<Cost>,
}

/// What a captured session cost, as the application reported it.
///
/// Both halves are optional in their own right: a run may report usage without
/// being priced, and the budget is measured in dollars while the usage is counted
/// in tokens. Neither is derived from the other here — converting between them
/// would bake a price list into the session file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    /// Tokens the application read.
    pub input_tokens: u64,
    /// Tokens the application wrote.
    pub output_tokens: u64,
    /// The priced total, when the application priced it.
    pub usd: Option<f64>,
}

/// An open session, appending frames as they settle and closing with a record.
pub struct SessionWriter {
    /// Where the record will be written when the session finishes.
    dir: PathBuf,
    /// The record as it stands; `finish` completes it and writes it out.
    record: SessionRecord,
    /// The frame stream.
    ///
    /// Buffered, and flushed on every append: see `append` for why the buffer is
    /// not allowed to hold a frame for long.
    frames: BufWriter<File>,
}

impl SessionWriter {
    /// Opens a session in `dir`, creating the directory if it is not there.
    ///
    /// The frame stream starts empty, and so does the directory: a record left by
    /// an earlier run is removed. A session directory describes exactly one capture,
    /// and reusing one must not leave the previous run's record beside this run's
    /// fresh stream — that directory would read as a complete session made of one
    /// capture's facts and another's frames, which is worse than either alone.
    ///
    /// An existing record is removed rather than refused. Reusing an output path is
    /// normal, and failing there would break an interrupted-and-retried capture for
    /// no reason the caller can act on. The record is *not* written here either:
    /// nothing is known about how this session ended until it does.
    ///
    /// The record goes first, and the stream is truncated second, which is the only
    /// order that survives a kill between the two. Removing the record first leaves
    /// no record beside the previous run's frames: `read_frames` rejects that, and
    /// `read_orphaned_frames` recovers it, so the directory reads as an interrupted
    /// capture — which is what it is. Truncating first would leave the old record
    /// beside an empty stream, and that reads as a perfectly valid session with no
    /// frames in it: one capture's facts wearing another's data, silently. Two
    /// syscalls cannot be made atomic here, so the window is given the meaning that
    /// cannot be mistaken for a finished session.
    pub fn new(dir: &Path, record: SessionRecord) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        // Ignore a missing record — the usual case, and the whole point of reuse
        // being safe — but do not swallow any other error: a record that exists and
        // cannot be deleted would leave two captures in one directory.
        match fs::remove_file(dir.join(SESSION_FILE)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let frames = BufWriter::new(File::create(dir.join(FRAMES_FILE))?);
        Ok(Self {
            dir: dir.to_path_buf(),
            record,
            frames,
        })
    }

    /// Appends one frame to the stream as a line, and flushes it.
    ///
    /// The flush is deliberate, and is the one performance decision in this file.
    /// Capture is killed by Ctrl-C at least as often as it ends by itself, and a
    /// session whose frames were still in a process buffer when the signal arrived
    /// is a session with no frames in it. A write per settled screen costs nothing
    /// beside the 150 ms of quiet that a screen takes to settle, and it buys the
    /// guarantee that everything already reported to be recorded *is* recorded.
    ///
    /// A frame is serialised to a line before it is written, so the stream cannot
    /// end up with an object spread over two lines: one append is one `write_all`
    /// of one complete line, and a kill in the middle can only ever leave the
    /// partial line that the reader already tolerates.
    pub fn append(&mut self, frame: &Frame) -> io::Result<()> {
        let mut line = serde_json::to_string(frame)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        line.push('\n');
        self.frames.write_all(line.as_bytes())?;
        self.frames.flush()
    }

    /// Ends the session: records the cost and the end time, writes the record, and
    /// returns it as written.
    ///
    /// The frames are flushed before the record is written, so a directory holding
    /// a record holds every frame the session appended. The record is what says the
    /// session ended; writing it while frames were still buffered would leave a
    /// finished-looking session that was missing its last screens.
    pub fn finish(mut self, cost: Option<Cost>) -> io::Result<SessionRecord> {
        self.frames.flush()?;
        let mut record = self.record;
        record.cost = cost;
        record.ended_at = Some(timestamp(SystemTime::now()));
        write_record(&self.dir.join(SESSION_FILE), &record)?;
        Ok(record)
    }
}

/// Writes the record as one JSON object, replacing whatever was there.
///
/// Written whole rather than appended to, because a record is a single value: half
/// a record is a session whose screen size and command are unknown, which is worse
/// than no record at all. The bytes go to a staging file first and are renamed into
/// place, so no reader ever sees a record that exists but is empty — the record's
/// presence means the session ended, and that has to be true the moment it appears.
/// A kill between the write and the rename leaves a `.tmp` file and no record, which
/// is the honest reading: the session never got to say how it ended.
fn write_record(path: &Path, record: &SessionRecord) -> io::Result<()> {
    let text = serde_json::to_string_pretty(record)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let staging = staging_path(path);
    let mut file = File::create(&staging)?;
    file.write_all(text.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    fs::rename(&staging, path)
}

/// Where a record is written before it is renamed into its real place.
fn staging_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Reads a session directory: its record, and every frame that parses.
///
/// A directory with no record is not a session this function can return: the
/// record is written only by `finish`, and a capture killed before it ended never
/// got to say what it was. That is reported as `NotFound` naming `session.json`, so
/// a caller can tell an incomplete capture — which still has frames worth reading,
/// via `read_orphaned_frames` — from a directory that was never a session at all.
/// The record is deliberately not synthesised: a partial session and a finished one
/// must stay distinguishable.
///
/// A line that is not a frame is skipped with a warning naming it, rather than
/// failing the read. Capture is killed mid-write more often than it is stopped
/// cleanly, and the usual casualty is the last line of the stream: half an object,
/// perhaps not even a whole character. Failing there would throw away a session's
/// worth of good frames over the one that never finished arriving. Nothing else in
/// a stream is expected to be unreadable, which is why the skip is loud: a warning
/// per line, then a count, so a silently shrinking session cannot go unnoticed.
///
/// The stream is read as bytes and decoded lossily per line for the same reason: a
/// write cut mid-character leaves bytes that are not UTF-8 at all, and reading the
/// file as text would fail on exactly the truncated tail this function exists to
/// survive.
pub fn read_frames(dir: &Path) -> io::Result<(SessionRecord, Vec<Frame>)> {
    let record_path = dir.join(SESSION_FILE);
    let text = match fs::read_to_string(&record_path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{}: no session record; the capture did not finish \
                     (read_orphaned_frames reads the frames it did write)",
                    record_path.display()
                ),
            ));
        }
        Err(e) => return Err(e),
    };
    let record: SessionRecord =
        serde_json::from_str(&text).map_err(|e| invalid_data(&record_path, &e))?;
    Ok((record, read_frame_lines(&dir.join(FRAMES_FILE))?))
}

/// Reads the frames of a capture that was killed before it could finish.
///
/// Capture is stopped by Ctrl-C at least as often as it ends by itself, and every
/// append is flushed for exactly that case: the frames written before the signal
/// arrived are the session. `read_frames` cannot return them, because there is no
/// record to return beside them and inventing one would make an incomplete capture
/// look like a finished one. This is the reader that says so — it takes the stream
/// on its own, with the same tolerance for an unreadable line as `read_frames`.
///
/// It returns `NotFound` when there is no `frames.jsonl` either: a directory with
/// neither file is not a capture in any state.
pub fn read_orphaned_frames(dir: &Path) -> io::Result<Vec<Frame>> {
    read_frame_lines(&dir.join(FRAMES_FILE))
}

/// Reads a frame stream line by line, keeping every line that parses as a frame.
fn read_frame_lines(frames_path: &Path) -> io::Result<Vec<Frame>> {
    let mut lines = BufReader::new(File::open(frames_path)?);
    let mut frames = Vec::new();
    let mut skipped = 0usize;
    let mut number = 0usize;
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        if lines.read_until(b'\n', &mut bytes)? == 0 {
            break;
        }
        number += 1;
        let line = String::from_utf8_lossy(&bytes);
        match serde_json::from_str::<Frame>(&line) {
            Ok(frame) => frames.push(frame),
            Err(e) => {
                skipped += 1;
                eprintln!(
                    "warning: {}:{}: skipping unreadable frame line: {}",
                    frames_path.display(),
                    number,
                    e
                );
            }
        }
    }
    if skipped > 0 {
        eprintln!(
            "warning: {}: skipped {} unreadable line(s)",
            frames_path.display(),
            skipped
        );
    }
    Ok(frames)
}

/// A parse failure as an I/O error naming the file it came from.
fn invalid_data(path: &Path, error: &serde_json::Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {}", path.display(), error),
    )
}

/// The current time as an RFC 3339 UTC timestamp, e.g. `2026-10-27T10:07:17Z`.
///
/// Formatted here rather than pulled from a date library. The record's timestamps
/// are read by a human and ordered by a script that needs nothing more than a
/// lexical sort, and RFC 3339 in UTC gives both; a dependency to print a date is a
/// dependency the rig does not otherwise need.
fn timestamp(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    let rest = seconds % 86_400;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month,
        day,
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    )
}

/// The civil date `days` after 1970-01-01, by Howard Hinnant's `civil_from_days`.
///
/// The arithmetic every date library hides: it shifts the epoch to a 400-year era
/// so leap years repeat exactly, then places the month from March. Named here
/// because `timestamp` is otherwise a function that reformats a number nobody
/// checks.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    /// Days from 0000-03-01 to 1970-01-01.
    const SHIFT: i64 = 719_468;
    /// Days in a 400-year era, which has a whole number of weeks.
    const ERA: i64 = 146_097;

    let z = days + SHIFT;
    let era = (if z >= 0 { z } else { z - (ERA - 1) }) / ERA;
    let day_of_era = z - era * ERA; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / ERA) / 365; // [0, 399]
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let month_from_march = (5 * day_of_year + 2) / 153; // [0, 11], March is 0
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1; // [1, 31]
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    }; // [1, 12]
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::frame_from_chars;
    use tempfile::tempdir;

    /// A minimal record: the facts a capture knows before it runs, and a stand-in
    /// start time.
    fn rec() -> SessionRecord {
        SessionRecord {
            version: 1,
            started_at: "2026-10-27T10:00:00Z".into(),
            ended_at: None,
            command: "grok".into(),
            args: vec!["--no-color".into()],
            rows: 2,
            cols: 2,
            env: vec![("TERM".into(), "xterm-256color".into())],
            app_version: Some("1.2.3".into()),
            cost: None,
        }
    }

    #[test]
    fn session_roundtrips_frames_and_cost() {
        let dir = tempdir().unwrap();
        let mut w = SessionWriter::new(dir.path(), rec()).unwrap();
        w.append(&frame_from_chars(0, 1, 2, &[vec!['a', 'b']]))
            .unwrap();
        w.append(&frame_from_chars(1, 1, 2, &[vec!['c', 'd']]))
            .unwrap();
        let finished = w
            .finish(Some(Cost {
                input_tokens: 10,
                output_tokens: 5,
                usd: Some(0.01),
            }))
            .unwrap();
        assert_eq!(finished.cost.unwrap().output_tokens, 5);
        assert!(
            finished.ended_at.is_some(),
            "finish must record when the session ended"
        );
        let (back, frames) = read_frames(dir.path()).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].seq, 1);
        assert_eq!(back.command, "grok");
    }

    #[test]
    fn session_file_is_one_json_object_per_line() {
        let dir = tempdir().unwrap();
        let mut w = SessionWriter::new(dir.path(), rec()).unwrap();
        w.append(&frame_from_chars(0, 1, 1, &[vec!['a']])).unwrap();
        w.finish(None).unwrap();
        let text = std::fs::read_to_string(dir.path().join("frames.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 1);
        serde_json::from_str::<Frame>(text.lines().next().unwrap()).unwrap();

        // An absent cost travels as a null, not as a missing field: every reader
        // of a session reads `cost` by name.
        let wire: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("session.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(wire["cost"], serde_json::Value::Null);
    }

    #[test]
    fn a_truncated_final_line_is_skipped_rather_than_failing_the_read() {
        // Capture can be killed by Ctrl-C mid-write; a partial last line must not
        // make an otherwise good session unreadable.
        let dir = tempdir().unwrap();
        let mut w = SessionWriter::new(dir.path(), rec()).unwrap();
        w.append(&frame_from_chars(0, 1, 1, &[vec!['a']])).unwrap();
        w.finish(None).unwrap();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join("frames.jsonl"))
            .unwrap();
        use std::io::Write;
        // Half an object, and the tail of a multi-byte character inside it: a
        // write cut mid-character leaves bytes that are not UTF-8 at all, which is
        // why the stream is decoded per line rather than read as one text file.
        f.write_all(b"{\"seq\":1,\"rows\":1,\"runs\":[{\"ch\":\"\xe2\x96")
            .unwrap();
        let (_, frames) = read_frames(dir.path()).unwrap();
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn a_frame_reaches_the_disk_before_the_session_ends() {
        // A killed capture never reaches `finish`, and never drops the writer
        // either: whatever is on disk when the signal arrives is the whole
        // session. A writer that buffered its frames until the end would leave
        // nothing behind. Read the stream through a second handle while the
        // writer is still open, which is what a kill leaves as the visible state.
        let dir = tempdir().unwrap();
        let mut w = SessionWriter::new(dir.path(), rec()).unwrap();
        w.append(&frame_from_chars(0, 1, 1, &[vec!['a']])).unwrap();
        let on_disk = std::fs::read_to_string(dir.path().join("frames.jsonl")).unwrap();
        assert_eq!(
            on_disk.lines().count(),
            1,
            "append must flush the frame it was given"
        );
        assert!(
            !dir.path().join("session.json").exists(),
            "the record describes an ended session, so it cannot exist mid-capture"
        );
        w.finish(None).unwrap();
        assert!(dir.path().join("session.json").exists());
        assert!(
            !dir.path().join("session.json.tmp").exists(),
            "the staging file must be renamed away, not left beside the record"
        );
    }

    #[test]
    fn reusing_a_directory_leaves_only_the_new_capture() {
        // Reusing an output directory is normal — a script that captures the same
        // command twice writes to the same path. What must not survive is the
        // previous run's record: a directory holding run A's record beside run B's
        // truncated stream reads as a complete session that never happened, and
        // says nothing to a caller about which capture it is looking at.
        let dir = tempdir().unwrap();

        let mut first = SessionWriter::new(dir.path(), rec()).unwrap();
        first
            .append(&frame_from_chars(0, 1, 1, &[vec!['a']]))
            .unwrap();
        first.finish(None).unwrap();
        assert!(
            dir.path().join("session.json").exists(),
            "the first capture finished, so its record is on disk"
        );

        let mut second = SessionWriter::new(dir.path(), rec()).unwrap();
        assert!(
            !dir.path().join("session.json").exists(),
            "reuse must drop the previous run's record; a directory describes one capture"
        );

        second
            .append(&frame_from_chars(0, 1, 1, &[vec!['b']]))
            .unwrap();
        second
            .append(&frame_from_chars(1, 1, 1, &[vec!['c']]))
            .unwrap();
        second.finish(None).unwrap();

        let (_, frames) = read_frames(dir.path()).unwrap();
        assert_eq!(
            frames,
            vec![
                frame_from_chars(0, 1, 1, &[vec!['b']]),
                frame_from_chars(1, 1, 1, &[vec!['c']]),
            ],
            "the directory must hold the second run's frames and only those"
        );
    }

    #[test]
    fn new_clears_the_old_record_before_it_truncates_the_stream() {
        // `new` is two separate syscalls, and a kill can land between them. The
        // order decides what that half-made directory means. Record first: no
        // record and the previous run's frames, which `read_frames` rejects and
        // `read_orphaned_frames` recovers, so it reads as an interrupted capture.
        // Stream first: the previous run's record beside a fresh empty stream, which
        // reads as a perfectly valid session that never happened — the old capture's
        // facts wearing this capture's frames. The outcome of a *successful* `new` is
        // the same either way, so the order has to be observed from the state the
        // first step leaves behind when the second one fails: `File::create` cannot
        // open a directory, so a directory sitting at `frames.jsonl` makes `new`
        // fail at the truncate — after the record step, if the record step is first.
        let dir = tempdir().unwrap();
        let mut first = SessionWriter::new(dir.path(), rec()).unwrap();
        first
            .append(&frame_from_chars(0, 1, 1, &[vec!['a']]))
            .unwrap();
        first.finish(None).unwrap();
        assert!(
            dir.path().join("session.json").exists(),
            "the first capture finished, so its record is on disk"
        );

        // Nothing in `new` can fail before the truncate here: the directory exists,
        // and the record exists and is deletable. So this failure is the truncate.
        // (`frames.jsonl` is the first run's file; a directory in its place is what
        // `File::create` cannot open.)
        std::fs::remove_file(dir.path().join("frames.jsonl")).unwrap();
        std::fs::create_dir(dir.path().join("frames.jsonl")).unwrap();
        let err = match SessionWriter::new(dir.path(), rec()) {
            Ok(_) => panic!("opening a directory as the frame stream must fail"),
            Err(e) => e,
        };
        assert!(
            matches!(
                err.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::IsADirectory
            ),
            "the failure must be the stream step's, for the assertion below to prove the \
             record step ran first: {err}"
        );

        assert!(
            !dir.path().join("session.json").exists(),
            "the previous run's record must already be gone when the stream step fails: \
             removing the record first is what stops a kill between the two steps from \
             leaving a valid-looking session made of one capture's record and a stream \
             that was truncated for another"
        );
    }

    #[test]
    fn a_capture_killed_before_finish_reads_as_orphaned_frames() {
        // A kill never reaches `finish`, so there is no record — and the frames are
        // still there, flushed one per append precisely so a kill preserves them.
        // A reader that can only open a finished session would throw them away.
        let dir = tempdir().unwrap();
        let mut w = SessionWriter::new(dir.path(), rec()).unwrap();
        w.append(&frame_from_chars(0, 1, 1, &[vec!['a']])).unwrap();
        w.append(&frame_from_chars(1, 1, 1, &[vec!['b']])).unwrap();
        // The kill: the writer is dropped without `finish`. Every append already
        // flushed its line, so closing the file changes nothing on disk.
        drop(w);
        assert!(!dir.path().join("session.json").exists());

        assert_eq!(
            read_orphaned_frames(dir.path()).unwrap(),
            vec![
                frame_from_chars(0, 1, 1, &[vec!['a']]),
                frame_from_chars(1, 1, 1, &[vec!['b']]),
            ],
            "the flushed frames of a killed capture are the session"
        );

        // The same directory is still not a complete session: an incomplete capture
        // has to stay distinguishable from a finished one, and the error has to say
        // which file is missing so the caller knows that is why.
        let err = read_frames(dir.path()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        let message = err.to_string();
        assert!(
            message.contains("session.json"),
            "the NotFound must name the record, so an incomplete capture is distinguishable \
             from a directory that is not a session: {message}"
        );
    }

    #[test]
    fn timestamps_are_rfc_3339_utc() {
        use std::time::Duration;
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        // 1728000000 s = 2024-10-04T00:00:00Z, sixty days short of a leap-year
        // boundary, which is where an off-by-one in the era arithmetic shows.
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_secs(1_728_000_000)),
            "2024-10-04T00:00:00Z"
        );
    }
}
