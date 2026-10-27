//! `ui-lm-capture` — records a child program's terminal output as settled frames.
//!
//! One subcommand, `record`: run a program on a pty and, every time its output has
//! been quiet for long enough that the screen is what the program finished drawing,
//! append that screen to a session. The session format belongs to `ui-lm-frames`;
//! what this binary decides is *when* a screen is a frame and what is true about it.
//!
//! The loop is a poll loop rather than a blocking read. [`Driver::read_some`]
//! reports `WouldBlock` while the child is alive and quiet, and that is exactly the
//! moment quiescence becomes possible, so the read and the settle check happen
//! together: every pass reads what has arrived, offers the count to the
//! [`Quiescer`], and takes a frame when the quiet stretch is long enough. Nothing
//! in the loop waits on the child, so a program that stops drawing cannot stop the
//! recorder.
//!
//! # How it ends
//!
//! - **The stream ends.** A pty reports EOF once the child is gone and the driver
//!   has closed its end, so `Ok(0)` from the reader is the child having exited. The
//!   screen on display at that moment is the last settled screen there will ever
//!   be — no more output can arrive — so it is framed even if the quiet stretch has
//!   not elapsed yet, and the session is finished with a record.
//! - **An interrupt.** Ctrl-C (and, on Windows, Ctrl-Break, which is how a capture
//!   script stops a capture it started in a process group of its own) sets a flag
//!   the loop checks each pass. The loop then stops and finishes the session the
//!   same way: an interrupted capture is a capture, and what it wrote before the
//!   signal arrived is the session. Only a kill that runs no code leaves the frames
//!   without a record, which is what `read_orphaned_frames` reads.
//!
//! The handler itself does one thing — set an atomic flag — because the record
//! cannot be written from a signal handler: allocating and writing a file there is
//! not async-signal-safe. The loop notices the flag within a poll interval, which
//! is milliseconds.

use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

use ui_lm_capture::{
    added_environment, child_environment, frame_from_screen, Driver, Quiescer, SessionConfig,
};
use ui_lm_frames::{timestamp, Frame, SessionRecord, SessionWriter};

/// How long the child's output must be quiet before the screen counts as settled.
///
/// A screen caught mid-repaint is a state the program never meant to show anyone,
/// so a frame is taken only once output has stopped for longer than a burst takes
/// to arrive. 150 ms is the plan's number: well above the gap between the writes of
/// one repaint, well below the pause a person would notice.
const QUIESCE_GAP: Duration = Duration::from_millis(150);

/// Size of one read from the pty.
const READ_BUFFER: usize = 64 * 1024;

/// How long the loop waits before asking a quiet child again.
///
/// Short enough that the settle gap is honoured to within a few milliseconds, long
/// enough that an idle session costs nothing.
const POLL_PAUSE: Duration = Duration::from_millis(5);

/// The longest delay a `--script` line may ask for, in milliseconds.
///
/// A delay is a pause before a keystroke, so the useful range is milliseconds to
/// seconds: one minute is far past any pause a capture script needs, and small
/// enough that no delay it accepts can overflow the clock. Anything longer is
/// refused as a script error naming its line — see [`split_delay`].
const MAX_DELAY_MILLIS: u64 = 60_000;

/// The command line: `ui-lm-capture record …`.
#[derive(Debug, Parser)]
#[command(
    name = "ui-lm-capture",
    version,
    about = "Records a terminal program as settled frames"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a program on a pty and record its screens as a session.
    Record(Record),
}

#[derive(Debug, Args)]
struct Record {
    /// Screen height, in rows.
    #[arg(long)]
    rows: u16,

    /// Screen width, in columns.
    #[arg(long)]
    cols: u16,

    /// Directory the session is written to: `session.json` and `frames.jsonl`.
    ///
    /// Created if it is not there. A directory describes one capture, so a record
    /// left by an earlier run is removed rather than merged in.
    #[arg(long)]
    out: PathBuf,

    /// The directory the program runs in.
    ///
    /// The recorder's own current directory unless this is given. A capture is
    /// about a directory — a scratch project, a fixture tree — and the rig must not
    /// leave that choice to `portable-pty`, whose default for a program it was given
    /// no directory for is the user's home directory. A path that is not a directory
    /// is refused rather than falling back, because falling back would land exactly
    /// where this flag exists to prevent.
    #[arg(long, value_name = "DIR")]
    cwd: Option<PathBuf>,

    /// A file of keystrokes to drive the program with, one per line.
    ///
    /// Each line is an optional delay in milliseconds, then the keystroke to send
    /// after it: `250 \x1b[B` sends a down-arrow 250 ms after the line before it,
    /// and `hello\r` types `hello` and presses Enter. A delay may be at most 60000
    /// ms. The escapes `\r`, `\n`, `\t`, `\e`, `\x41` and `\\` are interpreted;
    /// after any other backslash everything is literal, so a Windows path can be
    /// written as it is. Leading whitespace on a line is not sent, trailing
    /// whitespace is.
    #[arg(long, value_name = "FILE")]
    script: Option<PathBuf>,

    /// The program to run, and its arguments, after `--`.
    #[arg(last = true, required = true, num_args = 1.., value_name = "COMMAND")]
    command: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ui-lm-capture: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<ExitCode> {
    match command {
        Command::Record(args) => record(args),
    }
}

/// Records one session, and reports how it ended.
fn record(args: Record) -> Result<ExitCode> {
    if args.rows == 0 || args.cols == 0 {
        bail!(
            "a screen needs at least one row and one column, not {}x{}",
            args.rows,
            args.cols
        );
    }
    let (program, program_args) = split_command(&args.command)?;
    let mut keystrokes = match &args.script {
        Some(path) => read_script(path)?,
        None => VecDeque::new(),
    };
    signal::install();

    let cfg = SessionConfig {
        command: program.clone(),
        args: program_args.clone(),
        rows: args.rows,
        cols: args.cols,
        // The rig's own defaults are the environment of every session; the record
        // says so below, from the same helper the spawn uses rather than from a copy.
        env: Vec::new(),
        // `None` is not "wherever portable-pty likes": the driver resolves it to
        // this process's own directory.
        cwd: args.cwd,
    };
    // What the child will see and where it will run, resolved before the
    // session is opened: `working_directory` is the driver's own resolution (the
    // flag, or this recorder's directory), so the record cannot disagree with the
    // spawn, and a directory that is not one fails the run before anything is
    // created. `child_environment` is the rendering allowlist of the environment for
    // the same reason — the child inherits this process's environment and the
    // driver's defaults are applied over it, so the defaults alone are not what the
    // program ran with — and an allowlist rather than the environment itself
    // because a session is a shareable artefact and this recorder is run with
    // whatever credential the application needs exported.
    let child_cwd = cfg.working_directory()?;
    let child_env = child_environment(&cfg.env);
    // The record is known before the session runs, except for how it ended: the
    // writer completes it in `finish`.
    let session = SessionRecord {
        version: 1,
        started_at: timestamp(SystemTime::now()),
        ended_at: None,
        command: program,
        args: program_args,
        rows: args.rows,
        cols: args.cols,
        env: added_environment(&cfg.env),
        child_env,
        cwd: Some(child_cwd.display().to_string()),
        app_version: None,
        cost: None,
    };
    // Opened before the child is spawned, so a bad output directory fails the run
    // without launching a program behind it. A spawn that fails afterwards leaves an
    // empty stream and no record, which reads as an interrupted capture — the honest
    // description of what happened.
    let mut writer = SessionWriter::new(&args.out, session)
        .with_context(|| format!("failed to open the session in {}", args.out.display()))?;
    let mut driver = Driver::spawn(&cfg)?;

    let mut quiescer = Quiescer::new(QUIESCE_GAP);
    let mut previous: Option<Frame> = None;
    let mut buffer = vec![0u8; READ_BUFFER];
    let mut frames = 0usize;
    let mut refused = 0usize;
    let mut typed_since_frame = false;
    let mut unrecorded_output = false;
    // When the script's first keystroke is due, if it has one. `deadline_after`
    // rather than `Instant::now() + step.delay`: the bare addition panics on a delay
    // the clock cannot hold, and a panic here would end the capture before the child
    // had drawn anything.
    let mut next_keystroke_at = match keystrokes.front() {
        Some(step) => Some(deadline_after(Instant::now(), step.delay)?),
        None => None,
    };

    let ended = loop {
        if signal::interrupted() {
            break Ended::Interrupted;
        }

        let (bytes_read, stream_ended) = match driver.read_some(&mut buffer) {
            Ok(0) => (0, true),
            Ok(read) => (read, false),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => (0, false),
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read from {:?}", cfg.command))
            }
        };
        if bytes_read > 0 {
            unrecorded_output = true;
        }

        // A settled screen is one whose burst has been quiet for the gap. The end of
        // the stream settles the last screen by proof instead: the child is gone, so
        // nothing more will ever be drawn and what is on screen now is final.
        let settled =
            quiescer.observe(Instant::now(), bytes_read) || (stream_ended && unrecorded_output);
        if settled {
            emit_frame(
                &driver,
                &mut writer,
                &mut quiescer,
                &mut previous,
                typed_since_frame,
            )?;
            frames += 1;
            typed_since_frame = false;
            unrecorded_output = false;
        }

        if stream_ended {
            break Ended::StreamEnded;
        }

        // The script's next keystroke, if it is due — and after this pass's frame, so
        // that a keystroke queued now is never counted as having happened between two
        // frames whose second screen cannot have answered it yet.
        //
        // Sending is queueing: the driver never waits here, so a child that stopped
        // reading its input cannot stall the loop. A keystroke that cannot be queued is
        // a keystroke that was never delivered, and is counted rather than retried — a
        // queue that is full means the child is not collecting what it is sent, and
        // waiting for it is the hang this design exists to avoid.
        if let Some(due) = next_keystroke_at {
            if Instant::now() >= due {
                let step = keystrokes
                    .pop_front()
                    .expect("a due keystroke is one the script still has");
                match driver.write_input(&step.bytes) {
                    Ok(()) => typed_since_frame = true,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        refused += 1;
                        eprintln!(
                            "warning: the pty write queue was full, so scripted keystroke {} \
                             ({:?}) was never delivered",
                            step.number, step.bytes
                        );
                    }
                    Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                        // Every later keystroke would fail the same way: the session's
                        // write side has closed, so the child is gone or has stopped
                        // reading for good.
                        let lost = 1 + keystrokes.len();
                        refused += lost;
                        keystrokes.clear();
                        eprintln!(
                            "warning: the pty write side is closed, so scripted keystroke {} and \
                             the {lost} keystroke(s) after it were never delivered",
                            step.number
                        );
                    }
                    Err(error) => {
                        return Err(error).with_context(|| {
                            format!("failed to send scripted keystroke {}", step.number)
                        })
                    }
                }
                // The next keystroke's instant, if the script has another one — and
                // checked, so a delay the clock cannot hold is an error rather than
                // the panic that used to leave flushed frames with no record.
                next_keystroke_at = match keystrokes.front() {
                    Some(step) => Some(deadline_after(Instant::now(), step.delay)?),
                    None => None,
                };
            }
        }

        if bytes_read == 0 {
            std::thread::sleep(POLL_PAUSE);
        }
    };

    writer
        .finish(None)
        .context("failed to write the session record")?;
    // A cursor-position query is how a terminal application asks where the cursor
    // is, and it does not draw correctly until it has the answer. A reply that could
    // not be queued — the child had stopped reading, or its write side was gone —
    // leaves it waiting on an answer that will never arrive, so the screen it draws
    // afterwards is not the screen it meant to draw. The capture is still a capture:
    // the frames are what the emulator saw. What must not happen is a session like
    // that reporting success with nothing said about it, because every number
    // measured from those frames would be measuring the wrong screen.
    let dropped = driver.dropped_replies();
    if dropped > 0 {
        eprintln!(
            "warning: {dropped} cursor-position reply/replies the child asked for could not be \
             queued and were dropped (it had stopped reading its input); a child starved of those \
             answers draws a screen it did not mean to, so this session's later frames may not \
             be the application's own drawing"
        );
    }
    if refused > 0 {
        eprintln!(
            "warning: {refused} scripted keystroke(s) were refused by the pty and never reached the \
             child; the session records the screens the child drew, not the ones the script asked \
             for"
        );
    }
    // Keystrokes the loop never got to are a different failure from a refused one:
    // the session ended first, so the script ran short. A scenario script asks for a
    // session it described, and this says the program stopped before it had drawn it.
    if !keystrokes.is_empty() {
        eprintln!(
            "warning: the program stopped with {} scripted keystroke(s) still to send, so the \
             session is shorter than the script asked for",
            keystrokes.len()
        );
    }
    println!(
        "recorded {frames} frame(s) to {} ({})",
        args.out.display(),
        ended.describe(QUIESCE_GAP),
    );

    // The child's own exit status is reported, not propagated. This process's exit
    // code describes *this* process's job, and the job is done: the session was
    // written. A scenario that deliberately kills the application would otherwise
    // read as a failed capture to the script running it, and a capture that really
    // did fail needs no code to say so — it leaves no `session.json`. So the status
    // goes to stderr, where triage can still see it and where it cannot be mistaken
    // for the recorder's own verdict.
    if matches!(ended, Ended::StreamEnded) {
        if let Some(code) = driver.try_wait()? {
            if code != 0 {
                eprintln!("warning: {:?} exited with status {code}", cfg.command);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Why the record loop stopped.
#[derive(Clone, Copy, Debug)]
enum Ended {
    /// The child's output stream ended, so the program had exited.
    StreamEnded,
    /// An interrupt asked the capture to stop.
    Interrupted,
}

impl Ended {
    /// How an ending is described on stderr, since the two mean different things.
    fn describe(self, gap: Duration) -> String {
        match self {
            Self::StreamEnded => "the program exited".to_owned(),
            Self::Interrupted => {
                format!("interrupted; frames settled up to the last {gap:?} of quiet")
            }
        }
    }
}

/// Appends the screen as it now stands to the session, and makes it the screen the
/// next frame is diffed against.
///
/// What the frame says about the screen is decided by
/// [`ui_lm_capture::frame_from_screen`]; this appends it and carries it forward as
/// the diff baseline. The first frame is the one case that call site has to get
/// right: there is no frame before it, so `previous` stays `None` for the first
/// append and the diff below is against [`Frame::empty`], which is what makes every
/// row of a first screen new content.
fn emit_frame(
    driver: &Driver,
    writer: &mut SessionWriter,
    quiescer: &mut Quiescer,
    previous: &mut Option<Frame>,
    keystroke_between: bool,
) -> Result<()> {
    let frame = frame_from_screen(
        driver.screen(),
        quiescer.take_seq(),
        previous.as_ref(),
        keystroke_between,
    );
    writer
        .append(&frame)
        .with_context(|| format!("failed to append frame {}", frame.seq))?;
    *previous = Some(frame);
    Ok(())
}

/// The program and its arguments, from the trailing command line.
fn split_command(command: &[String]) -> Result<(String, Vec<String>)> {
    let Some((program, arguments)) = command.split_first() else {
        bail!(
            "no program to run: pass one after `--`, e.g. `ui-lm-capture record --rows 24 --cols \
             80 --out session -- cmd /c dir`"
        );
    };
    Ok((program.clone(), arguments.to_vec()))
}

/// One keystroke the script asks for.
struct Keystroke {
    /// Which line of the script it came from, for the messages about it.
    number: usize,
    /// How long to wait before sending it.
    delay: Duration,
    /// The bytes to write to the child's stdin.
    bytes: Vec<u8>,
}

/// Reads `path` as a keystroke script.
///
/// One keystroke per line, with an optional leading delay in milliseconds. The delay
/// is how a script waits for the application: the first line's delay is the pause
/// before the first keystroke, so `200 hello\r` waits 200 ms and then types. A blank
/// line asks for nothing and is skipped; so is a trailing newline.
fn read_script(path: &Path) -> Result<VecDeque<Keystroke>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read the keystroke script {}", path.display()))?;
    let mut steps = VecDeque::new();
    for (index, line) in text.lines().enumerate() {
        // `lines` drops the `\n` but keeps a `\r` from a CRLF file, which is a line
        // ending rather than part of the keystroke.
        let line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let (delay, payload) = split_delay(trimmed).with_context(|| {
            format!(
                "{}:{}: bad keystroke script line",
                path.display(),
                index + 1
            )
        })?;
        steps.push_back(Keystroke {
            number: index + 1,
            delay,
            bytes: unescape(payload),
        });
    }
    Ok(steps)
}

/// Splits a script line into its optional delay and its payload.
///
/// A leading run of digits followed by whitespace is the delay; anything else is
/// the payload from the first character on. Whitespace alone is not a delay, so a
/// keystroke that begins with digits can be written with a delay of its own.
///
/// The delay may be at most [`MAX_DELAY_MILLIS`]. A larger number is a script error
/// rather than a wait: it is either a unit typo (`99999999 hello\r` is 27 hours
/// written as if it were milliseconds), which would look exactly like a hung
/// recorder, or it is a delay whose sum with the clock does not fit, which is how a
/// capture used to end as a panic — mid-session, after frames had been written and
/// before the record that finishes them.
fn split_delay(line: &str) -> Result<(Duration, &str)> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let rest = &line[digits..];
    let separated = rest.chars().next().is_some_and(char::is_whitespace);
    if digits == 0 || !separated {
        return Ok((Duration::ZERO, line));
    }
    let milliseconds: u64 = line[..digits]
        .parse()
        .with_context(|| format!("{digits} digits is not a delay in milliseconds"))?;
    if milliseconds > MAX_DELAY_MILLIS {
        bail!(
            "a delay of {milliseconds} ms is longer than the {MAX_DELAY_MILLIS} ms maximum a \
             script line may ask for; a delay is how a script waits for the program to draw, \
             not how long the capture may take"
        );
    }
    Ok((
        Duration::from_millis(milliseconds),
        rest.trim_start_matches(char::is_whitespace),
    ))
}

/// The instant that a wait of `delay`, starting at `now`, ends at.
///
/// Never `now + delay`: that addition panics when the sum is past what the clock can
/// represent, and a panic in a wait site ends the capture without a session — the
/// exact unfinished-session failure that finishing on exit and on interrupt exists to
/// prevent. [`split_delay`]'s cap means no scripted delay can reach the overflow
/// today; this is the second guard, so that no arithmetic path panics even if the cap
/// is later raised.
fn deadline_after(now: Instant, delay: Duration) -> Result<Instant> {
    now.checked_add(delay).with_context(|| {
        format!(
            "a wait of {delay:?} is longer than this machine's clock can represent, so the \
             capture cannot schedule it"
        )
    })
}

/// The bytes a script line asks for, with its backslash escapes interpreted.
///
/// Only `\r`, `\n`, `\t`, `\e`, `\xNN` and `\\` are escapes; after any other
/// backslash the text is literal, which is what lets a script type a path or a
/// fragment of JSON without escaping it. A malformed `\x` is literal for the same
/// reason: sending the wrong bytes is worse than sending what was written.
fn unescape(text: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            bytes.extend_from_slice(character.encode_utf8(&mut [0u8; 4]).as_bytes());
            continue;
        }
        match characters.next() {
            Some('r') => bytes.push(b'\r'),
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some('e') => bytes.push(0x1b),
            Some('\\') => bytes.push(b'\\'),
            Some('x') => {
                let mut hex = String::new();
                while hex.len() < 2 {
                    match characters.peek() {
                        Some(digit) if digit.is_ascii_hexdigit() => {
                            hex.push(*digit);
                            characters.next();
                        }
                        _ => break,
                    }
                }
                match u8::from_str_radix(&hex, 16) {
                    Ok(byte) if hex.len() == 2 => bytes.push(byte),
                    _ => bytes.extend_from_slice(format!("\\x{hex}").as_bytes()),
                }
            }
            Some(other) => {
                bytes.push(b'\\');
                bytes.extend_from_slice(other.encode_utf8(&mut [0u8; 4]).as_bytes());
            }
            None => bytes.push(b'\\'),
        }
    }
    bytes
}

/// How the recorder learns that someone asked it to stop.
///
/// One flag, set from the handler and read by the loop. The handler does nothing
/// else because almost nothing is safe to do from one: `finish` allocates and writes
/// files, neither of which belongs in a signal handler. The loop polls the flag, so
/// the delay between the interrupt and the session being finished is one poll
/// pause.
#[cfg(unix)]
mod signal {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// `SIGINT`: what Ctrl-C delivers.
    const SIGINT: i32 = 2;
    /// `SIGTERM`: what a supervisor's polite stop delivers.
    const SIGTERM: i32 = 15;

    /// Whether an interrupt has been seen.
    static INTERRUPTED: AtomicBool = AtomicBool::new(false);

    extern "C" {
        /// `signal(3)`: install a handler for `signal`, returning the old one.
        fn signal(signal: i32, handler: extern "C" fn(i32)) -> usize;
    }

    /// The handler itself: record the interrupt and return, so the loop sees it and
    /// the session is finished from ordinary code.
    extern "C" fn note_interrupt(_signal: i32) {
        INTERRUPTED.store(true, Ordering::SeqCst);
    }

    /// Arranges for `SIGINT` and `SIGTERM` to be noticed rather than fatal.
    pub fn install() {
        // `signal` is async-signal-safe to call here, and the handler is a plain
        // function, so neither registration can fail in a way the caller could act
        // on.
        unsafe {
            signal(SIGINT, note_interrupt);
            signal(SIGTERM, note_interrupt);
        }
    }

    /// Whether an interrupt has arrived.
    pub fn interrupted() -> bool {
        INTERRUPTED.load(Ordering::SeqCst)
    }
}

/// How the recorder learns that someone asked it to stop, on a Windows console.
#[cfg(windows)]
mod signal {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// `CTRL_C_EVENT`: Ctrl-C.
    const CTRL_C_EVENT: u32 = 0;
    /// `CTRL_BREAK_EVENT`: Ctrl-Break, and how a script stops a capture it started
    /// in a process group of its own.
    const CTRL_BREAK_EVENT: u32 = 1;

    /// Whether an interrupt has been seen.
    static INTERRUPTED: AtomicBool = AtomicBool::new(false);

    extern "system" {
        /// `SetConsoleCtrlHandler`: the console's interrupt hook.
        fn SetConsoleCtrlHandler(handler: Option<extern "system" fn(u32) -> i32>, add: i32) -> i32;
    }

    /// The handler itself: record the interrupt and claim it, so Windows does not
    /// terminate the process before the session has been finished.
    ///
    /// The closing and logoff events are left to Windows: there is no time there to
    /// finish a session, and pretending otherwise would only lose the frames that
    /// are already on disk behind a record that never got written.
    extern "system" fn note_interrupt(event: u32) -> i32 {
        if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
            INTERRUPTED.store(true, Ordering::SeqCst);
            return 1;
        }
        0
    }

    /// Arranges for Ctrl-C and Ctrl-Break to be noticed rather than fatal.
    pub fn install() {
        // The handler has no state to share but the flag, so the call cannot fail in
        // a way the capture could compensate for.
        let installed = unsafe { SetConsoleCtrlHandler(Some(note_interrupt), 1) };
        if installed == 0 {
            eprintln!(
                "warning: no console control handler could be installed, so an interrupt will end \
                 this capture without a record: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    /// Whether an interrupt has arrived.
    pub fn interrupted() -> bool {
        INTERRUPTED.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_line_carries_its_delay_before_the_keystroke() {
        // The delay is how a script waits for the application to draw before typing
        // at it, which is the whole reason a script is not just a file of bytes.
        let out = tempfile::tempdir().unwrap();
        let script = out.path().join("keys.txt");
        // `\\r` in this literal is the script's two-character escape, not a line
        // ending: a real CR before the newline would be stripped as CRLF, which is
        // what a script checked out on Windows has.
        std::fs::write(&script, "250 \\x1b[B\nhello\\r\n\r\n10 42\\r\n").unwrap();
        let steps = read_script(&script).unwrap();
        let found: Vec<(u64, &[u8])> = steps
            .iter()
            .map(|step| (step.delay.as_millis() as u64, &step.bytes[..]))
            .collect();
        assert_eq!(
            found,
            vec![
                (250, &b"\x1b[B"[..]),
                (0, &b"hello\r"[..]),
                // A keystroke that begins with digits needs a delay of its own to be
                // read as one: without the separating space, "10" is the payload.
                (10, &b"42\r"[..]),
            ],
            "one keystroke per line, each with the delay written before it"
        );
    }

    #[test]
    fn digits_without_a_delay_separator_are_the_keystroke() {
        // A bare number is text a script may well want to type; only a run of digits
        // followed by whitespace is a delay.
        assert_eq!(split_delay("42").unwrap().0, Duration::ZERO);
        assert_eq!(split_delay("42").unwrap().1, "42");
        assert_eq!(split_delay("42 eggs").unwrap().0, Duration::from_millis(42));
    }

    #[test]
    fn a_delay_over_the_cap_is_refused_at_the_cap() {
        // A delay is a pause before a keystroke, so its useful range is
        // milliseconds to seconds. One minute is the largest a line may ask for:
        // exactly that is allowed, one millisecond more is a typo, and a typo has
        // to be a parse error rather than a wait so long the capture looks hung.
        assert_eq!(
            split_delay("60000 hello").unwrap().0,
            Duration::from_millis(60_000)
        );
        let error = split_delay("60001 hello")
            .expect_err("a delay over the cap is not a keystroke script line");
        let message = format!("{error:#}");
        assert!(
            message.contains("60001") && message.contains("60000"),
            "the error must name the delay that was asked for and the limit it broke: \
             {message:?}"
        );
    }

    #[test]
    fn an_absurd_delay_is_an_error_rather_than_a_panic() {
        // `Instant::now() + delay` panics when the sum does not fit in the clock, and
        // a panic in the wait sites ends a capture without a session. The script
        // parser's cap keeps a script from reaching that today; this guard is what
        // keeps it from being a panic if the cap is ever raised, so it is exercised
        // through the private helper rather than through a script.
        let now = Instant::now();
        assert!(
            deadline_after(now, Duration::MAX).is_err(),
            "a delay the clock cannot represent is an error, not a panic"
        );
        // A wait the clock can hold is still an ordinary instant.
        assert_eq!(
            deadline_after(now, Duration::from_millis(60_000)).unwrap(),
            now + Duration::from_millis(60_000)
        );
    }

    #[test]
    fn only_the_documented_escapes_are_interpreted() {
        // A capture script types paths and fragments of text, so an unknown escape
        // has to stay literal rather than becoming the wrong byte silently.
        assert_eq!(unescape(r"\x1b[A"), b"\x1b[A");
        assert_eq!(unescape(r"a\r\nb\t"), b"a\r\nb\t");
        assert_eq!(unescape(r"C:\Users\me"), br"C:\Users\me");
        assert_eq!(unescape(r"\\"), b"\\");
        // A `\x` that is not two hex digits is literal, including what it swallowed.
        assert_eq!(unescape(r"\x2"), b"\\x2");
        assert_eq!(unescape(r"\xzz"), br"\xzz");
    }
}
