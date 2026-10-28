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
    added_environment, child_environment, events, frame_from_screen, Driver, Quiescer,
    SessionConfig,
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
    /// Report what a set of recorded sessions contains.
    Summarize(Summarize),
    /// Check a corpus against the promises the volatile mask makes.
    Audit(Summarize),
    /// Print the page a recorded frame describes, as an A2UI message.
    Page(PageArgs),
    /// Score the learned model against the pages the recorder produces.
    Score(ScoreArgs),
    /// Draw a page back as terminal text.
    Tui(TuiArgs),
    /// Check the label path against the reader's own answers.
    SelfCheck(SelfCheckArgs),
    /// Check that pages carry provenance honestly across a sequence.
    Provenance(ProvenanceArgs),
    /// Score a page built from text alone against the pages the recorder produces.
    Baseline(BaselineArgs),
    /// Write the training pairs a recording holds: the screen's text, and its page.
    Dataset(DatasetArgs),
    /// Draw recorded pages as one standalone HTML file you can open in a browser.
    Render(RenderArgs),
    /// Score one recorded session's pages against another's.
    ///
    /// The use this exists for: record the same deterministic scenario twice and see
    /// whether the pages agree. A dataset whose structure changes between identical
    /// captures cannot support a claim about a model.
    Compare(CompareArgs),
}

/// Checks that a recorded sequence carries its provenance honestly.
#[derive(Debug, Args)]
struct ProvenanceArgs {
    /// Session directories holding frames.
    #[arg(long = "session", value_name = "DIR", required = true)]
    sessions: Vec<PathBuf>,
}
/// Feeds the label path the reader's own answers, to find where the two disagree.
#[derive(Debug, Args)]
struct SelfCheckArgs {
    /// Session directories holding frames.
    #[arg(long = "session", value_name = "DIR", required = true)]
    sessions: Vec<PathBuf>,

    /// Which frames to list, worst first.
    #[arg(long, value_name = "N", default_value_t = 0)]
    worst: usize,
}
/// What to score, and which model to score with.
#[derive(Debug, Args)]
struct ScoreArgs {
    /// Session directories holding frames.
    #[arg(long = "session", value_name = "DIR", required = true)]
    sessions: Vec<PathBuf>,

    /// The model to score, as written by `scripts/train_model.py`.
    #[arg(long, value_name = "FILE", default_value = "models/page.json")]
    model: PathBuf,

    /// Print up to this many components the model failed to reproduce, with their text.
    ///
    /// A score says how often the model is wrong; this says what it is wrong about, which is
    /// the only thing that decides whether the next change is data, features or labels.
    #[arg(long, value_name = "N", default_value_t = 0)]
    misses: usize,
}

/// Draws a recorded screen back from its page, and says how much survived the round trip.
#[derive(Debug, Args)]
struct TuiArgs {
    /// The session directory holding the frames.
    #[arg(long, value_name = "DIR")]
    session: PathBuf,

    /// Which frame, by sequence number. Defaults to the session's last frame.
    #[arg(long)]
    seq: Option<u32>,

    /// Draw every frame and report the round trip over all of them.
    #[arg(long)]
    all: bool,
}
/// Which recording to measure the floor on.
#[derive(Debug, Args)]
struct BaselineArgs {
    /// A session directory holding frames.
    #[arg(long = "session", value_name = "DIR", required = true)]
    sessions: Vec<PathBuf>,
}

/// Which recording to export, and where to put the pairs.
#[derive(Debug, Args)]
struct DatasetArgs {
    /// A session directory holding frames. Repeat it to export several at once.
    #[arg(long = "session", value_name = "DIR", required = true)]
    sessions: Vec<PathBuf>,

    /// The JSON Lines file to write: one pair per line, `input` and `target`.
    #[arg(long, value_name = "FILE")]
    out: PathBuf,

    /// Keep only screens that hold a component beyond plain text.
    ///
    /// Measured on real corpora: most settled screens are prose, and a pair whose
    /// target is one block of text teaches nothing that copying already gives. The pairs
    /// worth having are the minority, and they are the ones a converter fails on.
    #[arg(long)]
    structured: bool,

    /// Replace the account name, home directory and session identifiers in each pair.
    ///
    /// The frames are left as recorded, because they are evidence. The pair is what gets
    /// copied and shared, so it is the pair that gets redacted, visibly: `<home>`,
    /// `<user>`, `<id>`. What it does not do is hide the shape of a directory tree, so a
    /// redacted pair is not automatically safe to publish.
    #[arg(long)]
    scrub: bool,
}
/// What to draw, and where to put it.
#[derive(Debug, Args)]
struct RenderArgs {
    /// The session directory holding the frames.
    #[arg(long, value_name = "DIR")]
    session: PathBuf,

    /// Which frame, by sequence number. Defaults to the session's last frame.
    #[arg(long)]
    seq: Option<u32>,

    /// Draw every frame, in order, into one document.
    #[arg(long)]
    all: bool,

    /// The HTML file to write.
    #[arg(long, value_name = "FILE")]
    out: PathBuf,
}

/// The two sessions to score against each other.
#[derive(Debug, Args)]
struct CompareArgs {
    /// The session taken as the reference.
    #[arg(long, value_name = "DIR")]
    left: PathBuf,

    /// The session scored against it.
    #[arg(long, value_name = "DIR")]
    right: PathBuf,
}

/// Which frame to describe.
#[derive(Debug, Args)]
struct PageArgs {
    /// The session directory holding the frame.
    #[arg(long, value_name = "DIR")]
    session: PathBuf,

    /// Which frame, by sequence number. Defaults to the session's last frame.
    #[arg(long)]
    seq: Option<u32>,

    /// The surface id to write. Defaults to the session directory's own name.
    #[arg(long, value_name = "ID")]
    surface: Option<String>,

    /// Describe the frame with a trained model instead of the reader.
    #[arg(long, value_name = "FILE")]
    model: Option<PathBuf>,

    /// Describe every frame in order, one page per line, carrying provenance across
    /// them. Without this, one frame is described and every block in it is `seen`.
    #[arg(long)]
    all: bool,

    /// Print the model's label for each row instead of building a page.
    ///
    /// When a page groups rows differently from the reader, the question is always which row
    /// was labelled what; printing the labels answers it directly instead of by inference
    /// from the groups.
    #[arg(long)]
    labels: bool,
}

/// What the corpus reader is asked about.
#[derive(Debug, Args)]
struct Summarize {
    /// Session directories, or directories holding sessions, one or more.
    ///
    /// Both kinds are accepted because a corpus is usually pointed at from above —
    /// `sessions/` holds `w1/` and `k5/`, and both are wanted — while a single
    /// recording is pointed at directly.
    #[arg(last = true, required = true, num_args = 1.., value_name = "DIR")]
    sessions: Vec<PathBuf>,
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

    /// Root of the program's own event store, if it keeps one.
    ///
    /// Defaults to `<home>/.grok/sessions`. The recorder watches this recording's
    /// event stream there so it can tell a change the program reported from one
    /// nothing explains — the difference between a spinner tick and real content.
    /// Point it elsewhere to record another program, or leave the store out of
    /// reach: frames then carry no event range and the volatile mask stays empty,
    /// which is honest rather than wrong.
    #[arg(long, value_name = "DIR")]
    events: Option<PathBuf>,

    /// A file whose appended bytes are typed into the program while it runs.
    ///
    /// This is how something outside the recorder drives a live session: append to the
    /// file and the program receives those bytes, as if typed. A file rather than a
    /// socket on purpose — no port to open, nothing reachable from another machine, and
    /// the same behaviour on every platform. The recorder forwards only what is new since
    /// it last looked.
    #[arg(long, value_name = "FILE")]
    control: Option<PathBuf>,

    /// Stop the session after this many milliseconds, even if the program is still
    /// running.
    ///
    /// Without this a session ends when the program exits or when the recorder is
    /// killed, and a killed recorder leaves no `session.json` — the frames survive
    /// but the session reads as interrupted. A timed stop ends the loop normally, so
    /// the record is written and the run is reproducible: a capture that is meant to
    /// observe three seconds of a program should say three seconds, not wait for a
    /// scenario to talk the program into quitting.
    #[arg(long, value_name = "MILLIS")]
    stop_after: Option<u64>,
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
        Command::Summarize(args) => summarise(&args.sessions),
        Command::Audit(args) => audit(&args.sessions),
        Command::Page(args) => page(&args),
        Command::Compare(args) => compare(&args),
        Command::Baseline(args) => baseline(&args),
        Command::Score(args) => score(&args),
        Command::Tui(args) => tui(&args),
        Command::SelfCheck(args) => self_check(&args),
        Command::Provenance(args) => provenance(&args),
        Command::Dataset(args) => dataset(&args),
        Command::Render(args) => render(&args),
    }
}

/// Each component of a page as its kind and the text it carries.
///
/// Read off the serialized page rather than the typed structure so that every kind is
/// covered the same way — a diagnostic that skipped a kind would hide exactly the misses it
/// exists to find.
fn component_texts(page: &ui_lm_pages::Envelope) -> Vec<(String, String)> {
    let json = match serde_json::to_value(page) {
        Ok(json) => json,
        Err(_) => return Vec::new(),
    };
    let components = match json["createSurface"]["components"].as_array() {
        Some(components) => components,
        None => return Vec::new(),
    };
    components
        .iter()
        .filter_map(|component| {
            let kind = component["component"].as_str()?.to_owned();
            if kind == "Column" {
                return None;
            }
            let text = if let Some(lines) = component["lines"].as_array() {
                lines
                    .iter()
                    .map(|line| line.as_str().unwrap_or_default().to_owned())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else if let Some(calls) = component["calls"].as_array() {
                calls
                    .iter()
                    .map(|call| call["text"].as_str().unwrap_or_default().to_owned())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                component["text"].as_str().unwrap_or_default().to_owned()
            };
            Some((kind, text))
        })
        .collect()
}
/// Scores the learned model beside the do-nothing converter, on the same screens.
///
/// Two numbers side by side, because one alone says nothing: a model is only worth having
/// if it beats doing nothing, and the margin is the whole claim.
/// Copies the styling the recorder read onto a page the model judged.
///
/// The model decides what each row *is*; the cells decide how it *looks*. A page built from
/// labels alone had no spans at all — measured over the training batch, 6,303 spans from the
/// reader against 0 from the model — so every page the model described would have rendered in
/// one colour. Grafting them back keeps the division the project already states: judgement
/// comes from the model, construction from the code that watched the screen.
fn graft_spans(page: &mut ui_lm_pages::Envelope, reader: &ui_lm_pages::Envelope) {
    use ui_lm_pages::Body;
    let mut by_row: std::collections::HashMap<usize, Vec<ui_lm_pages::InlineSpan>> =
        Default::default();
    for component in &reader.create_surface.components {
        let (rows, spans) = match &component.body {
            Body::Block { rows, spans, .. } => (*rows, spans.clone()),
            Body::Diff { rows, lines } => (
                *rows,
                lines
                    .iter()
                    .map(|line| line.spans.clone())
                    .collect::<Vec<_>>(),
            ),
            _ => continue,
        };
        for (offset, line_spans) in spans.into_iter().enumerate() {
            by_row.insert(rows[0] as usize + offset, line_spans);
        }
    }
    for component in &mut page.create_surface.components {
        if let Body::Block {
            rows, spans, lines, ..
        } = &mut component.body
        {
            let first = rows[0] as usize;
            *spans = (0..lines.len())
                .map(|offset| by_row.get(&(first + offset)).cloned().unwrap_or_default())
                .collect();
        }
    }
}
fn tui(args: &TuiArgs) -> Result<ExitCode> {
    let frames = match ui_lm_frames::read_frames(&args.session) {
        Ok((_, frames)) => frames,
        Err(_) => ui_lm_frames::read_orphaned_frames(&args.session)?,
    };
    let chosen: Vec<&ui_lm_frames::Frame> = if args.all {
        frames.iter().collect()
    } else {
        vec![match args.seq {
            Some(seq) => frames
                .iter()
                .find(|frame| frame.seq == seq)
                .with_context(|| {
                    format!(
                        "the session {} has no frame {}",
                        args.session.display(),
                        seq
                    )
                })?,
            None => frames
                .last()
                .with_context(|| format!("{} holds no frames", args.session.display()))?,
        }]
    };
    let mut same = 0usize;
    let mut rows = 0usize;
    let mut drawn = 0usize;
    for frame in &chosen {
        let screen = ui_lm_capture::summary::frame_text(frame);
        let page = ui_lm_pages::page_from_frame(frame, "tui");
        let trip = ui_lm_render::round_trip(&page, &screen);
        same += trip.content_same;
        rows += trip.content;
        drawn += trip.drawn;
        if !args.all {
            print!("{}", ui_lm_render::render_terminal(&page));
        }
    }
    let share = if rows == 0 {
        100.0
    } else {
        same as f64 / rows as f64 * 100.0
    };
    eprintln!(
        "round trip: {same}/{rows} line(s) of content came back ({share:.1}%), {drawn} drawn"
    );
    Ok(ExitCode::SUCCESS)
}
fn provenance(args: &ProvenanceArgs) -> Result<ExitCode> {
    let mut checked = 0usize;
    let mut carried = 0usize;
    let mut errors = 0usize;
    for session in &args.sessions {
        let loaded = match ui_lm_frames::read_frames(session) {
            Ok((_, frames)) => frames,
            Err(_) => ui_lm_frames::read_orphaned_frames(session)?,
        };
        let surface = session
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "session".to_owned());
        // The rule: a component whose text has appeared in an earlier frame is `carried`,
        // and one whose text has not is `seen`. Anything else is the page claiming it read
        // something off the screen when it did not, or claiming newness for a repeat.
        let mut earlier: std::collections::HashSet<String> = Default::default();
        for page in ui_lm_pages::pages(&loaded, &surface) {
            for component in &page.create_surface.components {
                let text = match &component.body {
                    ui_lm_pages::Body::Column { .. } => continue,
                    ui_lm_pages::Body::Block { lines, .. } => lines.join("\n"),
                    ui_lm_pages::Body::Diff { lines, .. } => lines
                        .iter()
                        .map(|line| line.text.clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    ui_lm_pages::Body::ToolCall { calls, .. } => calls
                        .iter()
                        .map(|call| call.text.clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    ui_lm_pages::Body::Thought { text, .. } => text.clone(),
                    ui_lm_pages::Body::Meter { text, .. } => text.clone(),
                };
                checked += 1;
                let repeated = earlier.contains(&text);
                let says_carried = component.source == ui_lm_pages::Provenance::Carried;
                if says_carried {
                    carried += 1;
                }
                if repeated != says_carried {
                    errors += 1;
                }
                earlier.insert(text);
            }
        }
    }
    println!("provenance: {checked} component(s), {carried} carried, {errors} contradiction(s)");
    Ok(if errors == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
fn self_check(args: &SelfCheckArgs) -> Result<ExitCode> {
    let mut total = 0.0f64;
    let mut frames = 0usize;
    let mut worst: Vec<(f64, String, String)> = Vec::new();
    for session in &args.sessions {
        let loaded = match ui_lm_frames::read_frames(session) {
            Ok((_, frames)) => frames,
            Err(_) => ui_lm_frames::read_orphaned_frames(session)?,
        };
        for frame in &loaded {
            let surface = format!("frame-{}", frame.seq);
            let text = ui_lm_capture::summary::frame_text(frame);
            if text.trim().is_empty() {
                continue;
            }
            let lines: Vec<String> = text.lines().map(|line| line.to_owned()).collect();
            let real = ui_lm_pages::page_from_frame(frame, &surface);
            // The reader's own answer, one label per row: whatever kind it built for a row,
            // and `Block` for rows no component claims. If the label path is faithful, giving
            // it these labels must rebuild the same page — so every point of difference is a
            // place where the label path loses something the reader keeps.
            let mut labels = vec!["Block".to_owned(); lines.len()];
            for component in &real.create_surface.components {
                if let Some((first, end)) = rows_of_component(component) {
                    for row in first..end.min(labels.len()) {
                        labels[row] = component.component.clone();
                    }
                }
            }
            let rebuilt = ui_lm_pages::page_from_labels(&lines, &labels, &surface);
            let score = ui_lm_pages::compare(&real, &rebuilt).weighted_structure();
            total += score;
            frames += 1;
            if score < 1.0 {
                worst.push((score, session.display().to_string(), surface));
            }
        }
    }
    if frames == 0 {
        bail!("no frames to check");
    }
    println!(
        "label path given the reader's own answers: {:.1}% over {} frame(s)",
        total / frames as f64 * 100.0,
        frames
    );
    worst.sort_by(|left, right| left.0.total_cmp(&right.0));
    for (score, session, surface) in worst.iter().take(args.worst) {
        println!("  {:.1}%  {session} {surface}", score * 100.0);
    }
    Ok(ExitCode::SUCCESS)
}

/// The rows a component says it covers, when it says.
fn rows_of_component(component: &ui_lm_pages::Component) -> Option<(usize, usize)> {
    use ui_lm_pages::Body;
    let rows = match &component.body {
        Body::Column { .. } => return None,
        Body::Block { rows, .. }
        | Body::Diff { rows, .. }
        | Body::ToolCall { rows, .. }
        | Body::Thought { rows, .. }
        | Body::Meter { rows, .. } => *rows,
    };
    Some((rows[0] as usize, rows[1] as usize))
}
fn score(args: &ScoreArgs) -> Result<ExitCode> {
    let model = ui_lm_model::Model::load(&args.model)?;
    let mut ours = (0.0f64, 0usize);
    let mut floor = (0.0f64, 0usize);
    let mut kinds: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut misses: Vec<String> = Vec::new();
    let mut frames = 0usize;

    for session in &args.sessions {
        let path = session.as_path();
        let loaded = match ui_lm_frames::read_frames(path) {
            Ok((_, frames)) => frames,
            Err(_) => ui_lm_frames::read_orphaned_frames(path)?,
        };
        for frame in &loaded {
            let surface = format!("frame-{}", frame.seq);
            let text = ui_lm_capture::summary::frame_text(frame);
            if text.trim().is_empty() {
                continue;
            }
            let real = ui_lm_pages::page_from_frame(frame, &surface);
            let predicted = model.page(&text, &surface);
            let naive = ui_lm_pages::page_from_text(&text, &surface);
            ours.0 += ui_lm_pages::compare(&real, &predicted).weighted_structure();
            ours.1 += 1;
            floor.0 += ui_lm_pages::compare(&real, &naive).weighted_structure();
            floor.1 += 1;
            for kind in ui_lm_pages::compare(&real, &predicted).by_kind {
                let entry = kinds.entry(kind.kind).or_insert((0, 0));
                entry.0 += kind.total;
                entry.1 += kind.matched;
            }
            // What the model got wrong, named. A score says how often; this says what.
            if misses.len() < args.misses {
                for (kind, text) in component_texts(&real) {
                    if !component_texts(&predicted)
                        .iter()
                        .any(|(other_kind, other)| *other_kind == kind && *other == text)
                    {
                        misses.push(format!("{kind}: {text}"));
                        if misses.len() >= args.misses {
                            break;
                        }
                    }
                }
            }
            frames += 1;
        }
    }
    if frames == 0 {
        bail!("no frames to score");
    }
    let count = frames as f64;
    println!(
        "model {:.1}%  floor {:.1}%  over {} frame(s), trained on {} screen(s)",
        ours.0 / count * 100.0,
        floor.0 / count * 100.0,
        frames,
        model.trained_on
    );
    for (kind, (total, matched)) in &kinds {
        println!("  {kind:<10} {matched}/{total} reproduced");
    }
    for miss in &misses {
        println!("  missed {miss}");
    }
    Ok(ExitCode::SUCCESS)
}

/// Scores the do-nothing converter: text in, paragraphs out.
///
/// The number this prints is the floor. Anything that claims to understand a screen — a
/// model, a better set of rules — has to beat it, and without it "the model works" is a
/// feeling rather than a measurement.
fn baseline(args: &BaselineArgs) -> Result<ExitCode> {
    let mut structure = (0.0f64, 0usize);
    let mut text = (0.0f64, 0usize);
    let mut weighted = (0.0f64, 0usize);
    let mut structured = (0.0f64, 0usize);
    let mut plain = 0usize;
    let mut kinds: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for session in &args.sessions {
        let path = session.as_path();
        let frames = match ui_lm_frames::read_frames(path) {
            Ok((_, frames)) => frames,
            Err(_) => ui_lm_frames::read_orphaned_frames(path)?,
        };
        for frame in &frames {
            let surface = format!("frame-{}", frame.seq);
            let text_of_screen = ui_lm_capture::summary::frame_text(frame);
            let naive = ui_lm_pages::page_from_text(&text_of_screen, &surface);
            let real = ui_lm_pages::page_from_frame(frame, &surface);
            let score = ui_lm_pages::compare(&real, &naive);
            structure.0 += score.structure();
            structure.1 += 1;
            text.0 += score.text();
            text.1 += 1;
            weighted.0 += score.weighted_structure();
            // A frame whose page holds only plain text is trivial to reproduce, and a
            // plain average over frames lets those drown out the screens that carry
            // structure. They are counted apart, because "78% overall" is meaningless if
            // most screens were empty.
            if score.by_kind.iter().any(|kind| kind.kind != "Block") {
                structured.0 += score.weighted_structure();
                structured.1 += 1;
            } else {
                plain += 1;
            }
            for kind in &score.by_kind {
                let entry = kinds.entry(kind.kind.clone()).or_insert((0usize, 0usize));
                entry.0 += kind.total;
                entry.1 += kind.matched;
            }
        }
    }
    if structure.1 == 0 {
        bail!("no frames to measure in {} session(s)", args.sessions.len());
    }
    let count = structure.1 as f64;
    println!(
        "floor over {} frame(s): structure {:.1}%, weighted {:.1}%, text {:.1}%",
        structure.1,
        structure.0 / count * 100.0,
        weighted.0 / count * 100.0,
        text.0 / count * 100.0
    );
    let structured_avg = if structured.1 == 0 {
        0.0
    } else {
        structured.0 / structured.1 as f64 * 100.0
    };
    println!(
        "  of {} frame(s), {plain} held only plain text and {} carried structure",
        structure.1, structured.1
    );
    if structured.1 > 0 {
        println!("  floor on the screens that carry structure: {structured_avg:.1}%");
    }
    for (kind, (total, matched)) in &kinds {
        println!(
            "  {kind:<10} {matched}/{total} reproduced ({:.0}%)",
            f64::from(u32::try_from(*matched).unwrap_or(0))
                / f64::from(u32::try_from(*total).unwrap_or(1))
                * 100.0
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Writes the training pairs a recording holds.
fn dataset(args: &DatasetArgs) -> Result<ExitCode> {
    let mut pairs = Vec::new();
    // A screen that has not changed is not a second example. Recording settles often, so a
    // session can hold the same quiet screen several times; exporting it several times
    // makes a corpus look bigger and teaches less, because a model would be rewarded for
    // repeating the commonest screen in it.
    let mut seen: std::collections::HashSet<String> = Default::default();
    let mut repeated = 0usize;
    for session in &args.sessions {
        let found = ui_lm_capture::summary::training_pairs(session, args.structured)
            .with_context(|| format!("failed to read the session {}", session.display()))?;
        if found.is_empty() {
            eprintln!(
                "warning: {} holds no screen with anything on it",
                session.display()
            );
        }
        for (input, target) in found {
            if seen.insert(input.clone()) {
                pairs.push((input, target));
            } else {
                repeated += 1;
            }
        }
    }
    if pairs.is_empty() {
        bail!(
            "none of the {} session(s) holds a screen with anything on it",
            args.sessions.len()
        );
    }
    // "Structured" turned out to be a weak word: the application draws a shortcut strip on
    // nearly every screen, so almost everything qualifies. What is actually rare is a screen
    // holding a component the text alone does not reveal, and that number is worth printing
    // next to the other rather than leaving a reader to assume the first means the second.
    let deep = pairs
        .iter()
        .filter(|(_, target)| {
            serde_json::from_str::<serde_json::Value>(target)
                .ok()
                .and_then(|page| {
                    page["createSurface"]["components"]
                        .as_array()
                        .map(|components| {
                            components.iter().any(|component| {
                                !matches!(component["component"].as_str(), Some("Block" | "Column"))
                            })
                        })
                })
                .unwrap_or(false)
        })
        .count();
    println!(
        "{deep} of {} pair(s) hold a meter, a tool call, a diff or a reasoning line",
        pairs.len()
    );
    if repeated > 0 {
        println!("{repeated} repeated screen(s) left out");
    }

    if args.scrub {
        let scrubber = ui_lm_capture::scrub::Scrubber::for_this_machine();
        pairs = pairs
            .into_iter()
            .map(|(input, target)| (scrubber.apply(&input), scrubber.apply(&target)))
            .collect();
    } else {
        let personal = pairs
            .iter()
            .filter(|(input, target)| {
                ui_lm_capture::scrub::mentions_account(input)
                    || ui_lm_capture::scrub::mentions_account(target)
            })
            .count();
        if personal > 0 {
            eprintln!(
                "warning: {personal} of {} pair(s) name this machine's account; pass \
                 --scrub before sharing them",
                pairs.len()
            );
        }
    }
    use std::io::Write as _;
    let mut out = std::fs::File::create(&args.out)
        .with_context(|| format!("failed to create {}", args.out.display()))?;
    for (input, target) in &pairs {
        // One JSON object per line, so a trainer can stream it without holding the whole
        // set in memory, and a person can read a single example with `head -1`.
        let line = serde_json::json!({ "input": input, "target": target });
        writeln!(out, "{line}")?;
    }
    println!("wrote {} pair(s) to {}", pairs.len(), args.out.display());
    Ok(ExitCode::SUCCESS)
}

/// Draws recorded pages into one HTML file.
fn render(args: &RenderArgs) -> Result<ExitCode> {
    let pages = ui_lm_capture::summary::session_pages(&args.session)
        .with_context(|| format!("failed to read the session {}", args.session.display()))?;
    if pages.is_empty() {
        bail!(
            "the session {} holds no frames to draw",
            args.session.display()
        );
    }
    let title = args
        .session
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session".to_owned());

    let html = if args.all {
        ui_lm_render::render_pages(&pages, &title)
    } else {
        // A session's frames are numbered in order, and `session_pages` maps them in
        // that order, so a frame number is a position in this list.
        let index = match args.seq {
            Some(seq) => usize::try_from(seq)
                .ok()
                .filter(|index| *index < pages.len())
                .with_context(|| format!("the session has no frame {seq}"))?,
            None => pages.len() - 1,
        };
        ui_lm_render::render_page(&pages[index], &title)
    };
    std::fs::write(&args.out, &html)
        .with_context(|| format!("failed to write {}", args.out.display()))?;
    println!(
        "wrote {} ({} page(s), {} bytes)",
        args.out.display(),
        if args.all { pages.len() } else { 1 },
        html.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// Scores one session's pages against another's, reporting both kinds of agreement.
fn compare(args: &CompareArgs) -> Result<ExitCode> {
    let left = ui_lm_capture::summary::session_pages(&args.left)
        .with_context(|| format!("failed to read the session {}", args.left.display()))?;
    let right = ui_lm_capture::summary::session_pages(&args.right)
        .with_context(|| format!("failed to read the session {}", args.right.display()))?;
    if left.is_empty() || right.is_empty() {
        bail!("both sessions need at least one frame to be compared");
    }

    // Frames are aligned by position: two captures of one scenario should settle the
    // same screens in the same order, so the nth screen of one is the nth of the other.
    // A differing count is reported rather than hidden inside an average.
    let paired = left.len().min(right.len());
    let (mut structure, mut text) = (0.0, 0.0);
    for index in 0..paired {
        let score = ui_lm_pages::compare(&left[index], &right[index]);
        structure += score.structure();
        text += score.text();
    }
    let counted = paired as f64;
    println!(
        "compared {paired} frame(s): structure {:.1}%, text {:.1}%",
        structure / counted * 100.0,
        text / counted * 100.0
    );
    if left.len() != right.len() {
        println!(
            "note: {} frame(s) on the left, {} on the right; only the first {paired} were \
             compared",
            left.len(),
            right.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Prints the page one recorded frame describes.
///
/// A killed capture is readable here for the same reason it is summarisable: its
/// frames are real screens, and the last one is often the most interesting.
fn page(args: &PageArgs) -> Result<ExitCode> {
    let frames = match ui_lm_frames::read_frames(&args.session) {
        Ok((_, frames)) => frames,
        Err(_) => ui_lm_frames::read_orphaned_frames(&args.session)
            .with_context(|| format!("failed to read the session {}", args.session.display()))?,
    };
    let frame = match args.seq {
        Some(seq) => frames
            .iter()
            .find(|frame| frame.seq == seq)
            .with_context(|| format!("the session has no frame {}", seq))?,
        None => frames
            .last()
            .with_context(|| format!("the session {} holds no frames", args.session.display()))?,
    };
    let surface = args.surface.clone().unwrap_or_else(|| {
        args.session
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "session".to_owned())
    });
    if args.labels {
        let model = match &args.model {
            Some(path) => ui_lm_model::Model::load(path)?,
            None => bail!("--labels needs --model"),
        };
        let text = ui_lm_capture::summary::frame_text(frame);
        let lines = text.lines().map(|line| line.to_owned()).collect::<Vec<_>>();
        for (row, label) in model.labels(&lines).iter().enumerate() {
            println!("{row:>3} {label:<10} {}", lines[row].trim_end());
        }
        return Ok(ExitCode::SUCCESS);
    }
    // A whole session is printed as JSON lines, one page per frame: it is a stream
    // to be read in order, and indenting every page in a stream only costs bytes.
    if args.all {
        // `--all` with `--model` used to print the reader's pages and say nothing about it,
        // which is the worst kind of wrong: identical output to a request that never
        // mentioned a model. It now describes every frame with the model, and keeps
        // provenance across them the same way the reader does.
        if let Some(path) = &args.model {
            let model = ui_lm_model::Model::load(path)?;
            let mut appeared: std::collections::HashSet<String> = Default::default();
            for frame in &frames {
                let text = ui_lm_capture::summary::frame_text(frame);
                let mut page = model.page(&text, &surface);
                graft_spans(&mut page, &ui_lm_pages::page_from_frame(frame, &surface));
                for component in &mut page.create_surface.components {
                    let first_time = appeared.insert(component.id.clone());
                    component.source = if first_time {
                        ui_lm_pages::Provenance::Seen
                    } else {
                        ui_lm_pages::Provenance::Carried
                    };
                }
                let line =
                    serde_json::to_string(&page).context("failed to serialise a page as JSON")?;
                println!("{line}");
            }
            return Ok(ExitCode::SUCCESS);
        }
        for envelope in ui_lm_pages::pages(&frames, &surface) {
            let line =
                serde_json::to_string(&envelope).context("failed to serialise a page as JSON")?;
            println!("{line}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    // With a model, the frame is described by what the model reads from the screen's text
    // rather than by the reader. Both go through the same page builder, so a difference
    // between them is a difference in judgement and nothing else.
    let envelope = match &args.model {
        Some(path) => {
            let model = ui_lm_model::Model::load(path)?;
            model.page(&ui_lm_capture::summary::frame_text(frame), &surface)
        }
        None => ui_lm_pages::page_from_frame(frame, &surface),
    };
    let json =
        serde_json::to_string_pretty(&envelope).context("failed to serialise the page as JSON")?;
    println!("{json}");
    Ok(ExitCode::SUCCESS)
}

/// Checks a corpus against the promises the frames and the pages make, and fails if
/// either breaks one.
fn audit(sessions: &[PathBuf]) -> Result<ExitCode> {
    let context = || format!("failed to read the sessions under {sessions:?}");
    let mut violations = ui_lm_capture::summary::audit(sessions).with_context(context)?;
    violations.extend(ui_lm_capture::summary::audit_pages(sessions).with_context(context)?);
    if violations.is_empty() {
        println!("audit: no violations");
        return Ok(ExitCode::SUCCESS);
    }
    for violation in &violations {
        println!(
            "{}: frame {}: {}",
            violation.session, violation.seq, violation.problem
        );
    }
    println!("audit: {} violation(s)", violations.len());
    Ok(ExitCode::FAILURE)
}

/// Prints what the given sessions hold.
fn summarise(sessions: &[PathBuf]) -> Result<ExitCode> {
    let summary = ui_lm_capture::summary::summarize(sessions)
        .with_context(|| format!("failed to read the sessions under {sessions:?}"))?;
    print!("{}", summary.render());
    Ok(ExitCode::SUCCESS)
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
    let mut control = args
        .control
        .clone()
        .map(ui_lm_capture::control::ControlFile::new);
    if let Some(path) = &args.control {
        println!(
            "typing is open: bytes appended to {} are sent to the program",
            path.display()
        );
    }
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
    // The program's own event stream, when it keeps one. The tap resolves the
    // session directory lazily, because the application creates it after it
    // starts, so this is cheap here and may find nothing for the first few polls.
    let mut tap = args
        .events
        .clone()
        .or_else(events::default_store)
        .map(|store| events::EventTap::new(store, child_cwd.clone()));
    // Where the stream stood when the last frame was emitted. `None` means no
    // frame has been emitted yet; the first frame is unclassified regardless.
    let mut stream_at_previous_frame: Option<u64> = None;
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

    // The clock a frame's `at_ms` is measured from: the moment the recording loop
    // starts, which is as close to "the session began" as the recorder can know.
    let started = Instant::now();
    let stop_after = args.stop_after.map(Duration::from_millis);
    let mut next_control_at = Instant::now();

    let ended = loop {
        if signal::interrupted() {
            break Ended::Interrupted;
        }
        // A timed stop is checked first: a session that was asked to run for a fixed
        // span should end on that span even if output is still arriving, and what has
        // settled so far is exactly what it was asked to observe.
        if let Some(limit) = stop_after {
            if started.elapsed() >= limit {
                break Ended::Stopped;
            }
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
            // Did the program report anything since the frame before this one? A
            // longer event stream says yes. Where the stream cannot be found the
            // answer is "assume it may have", which keeps the mask from calling
            // real content noise.
            let stream_now = tap.as_mut().and_then(|tap| tap.length());
            // The range alone carries the answer: `frame_from_screen` reads "grew" as
            // "the program reported something", "did not grow" as "it reported
            // nothing", and "no range at all" as "nobody can say".
            let event_range = match (stream_now, stream_at_previous_frame) {
                (Some(now), Some(before)) => Some((before, now)),
                (Some(now), None) => Some((0, now)),
                (None, _) => None,
            };
            // What the application said between the last frame and this one, by its
            // own labels. Only meaningful with a range; without one there is nothing
            // honest to name, so the frame carries no kinds rather than an empty claim.
            let event_kinds = match (tap.as_mut(), event_range) {
                (Some(tap), Some((before, now))) if now > before => tap.kinds_between(before, now),
                _ => Vec::new(),
            };
            emit_frame(
                &driver,
                &mut writer,
                &mut quiescer,
                &mut previous,
                FrameNote {
                    keystroke_between: typed_since_frame,
                    acp_event_range: event_range,
                    at_ms: Some(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
                    event_kinds,
                },
            )?;
            if let Some(now) = stream_now {
                stream_at_previous_frame = Some(now);
            }
            frames += 1;
            typed_since_frame = false;
            unrecorded_output = false;
        }

        if stream_ended {
            break Ended::ProgramExited;
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
        // Typing from outside, at whatever moment the caller appends. Checked on a short
        // cadence rather than every pass: the loop spins fast, and a file read per spin
        // would cost more than the frame it is driving.
        if let Some(channel) = control.as_mut() {
            if Instant::now() >= next_control_at {
                next_control_at = Instant::now() + Duration::from_millis(20);
                match channel.take() {
                    Ok(bytes) if !bytes.is_empty() => match driver.write_input(&bytes) {
                        Ok(()) => typed_since_frame = true,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            refused += 1;
                            eprintln!(
                                "warning: the pty write queue was full, so {} byte(s) typed \
                                 into the control file were never delivered",
                                bytes.len()
                            );
                        }
                        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                            eprintln!(
                                "warning: the pty write side is closed, so typed input was \
                                 dropped"
                            );
                        }
                        Err(error) => {
                            return Err(error).with_context(|| {
                                "failed to send bytes from the control file".to_owned()
                            })
                        }
                    },
                    Ok(_) => {}
                    Err(error) => eprintln!(
                        "warning: could not read the control file {}: {error}",
                        args.control
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ),
                }
            }
        }

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
    if matches!(ended, Ended::ProgramExited) {
        // The stream ending usually means the child has exited, but "closed its
        // output" and "was reaped" are not the same instant: asking once loses the
        // status on a loaded machine, and the warning this exists to print then goes
        // missing. A short wait costs nothing and makes the report reliable.
        let mut status = driver.try_wait()?;
        let deadline = Instant::now() + Duration::from_millis(500);
        while status.is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            status = driver.try_wait()?;
        }
        if let Some(code) = status {
            if code != 0 {
                eprintln!("{}", exit_warning(std::slice::from_ref(&cfg.command), code));
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// How the child's own exit is reported on stderr.
///
/// The status is whatever the system reported, including the cases that are not an exit
/// code at all. On Windows a console child can be terminated by a console control event
/// and then reports `0xC000013A` — which is how a `cmd /c "… & exit 3"` occasionally ended
/// on a loaded machine, and which a person reading the warning deserves to have named
/// rather than decoded.
fn exit_warning(command: &[String], code: i32) -> String {
    // -1073741510 is 0xC000013A as a signed 32-bit integer: STATUS_CONTROL_C_EXIT.
    if code == -1073741510 {
        return format!(
            "warning: {command:?} was terminated by a console control event rather than \
             exiting with a status of its own"
        );
    }
    format!("warning: {command:?} exited with status {code}")
}
/// Why the record loop stopped.
#[derive(Clone, Copy, Debug)]
enum Ended {
    /// The child's output stream ended, so the program had exited.
    ProgramExited,
    /// An interrupt asked the capture to stop.
    Interrupted,
    /// The session reached the span `--stop-after` asked for.
    Stopped,
}

impl Ended {
    /// How an ending is described on stderr, since the two mean different things.
    fn describe(self, gap: Duration) -> String {
        match self {
            Self::ProgramExited => "the program exited".to_owned(),
            Self::Interrupted => {
                format!("interrupted; frames settled up to the last {gap:?} of quiet")
            }
            Self::Stopped => "reached the span `--stop-after` asked for".to_owned(),
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
/// What the frame about to be written should say besides what is on screen.
struct FrameNote {
    /// Whether the recorder sent the program a keystroke since the last frame.
    keystroke_between: bool,
    /// The stretch of the event stream this frame sits at the end of.
    acp_event_range: Option<(u64, u64)>,
    /// Milliseconds since the session started.
    at_ms: Option<u64>,
    /// The application's own labels for the events in that stretch.
    event_kinds: Vec<String>,
}

fn emit_frame(
    driver: &Driver,
    writer: &mut SessionWriter,
    quiescer: &mut Quiescer,
    previous: &mut Option<Frame>,
    note: FrameNote,
) -> Result<()> {
    let frame = frame_from_screen(
        driver.screen(),
        quiescer.take_seq(),
        previous.as_ref(),
        note.keystroke_between,
        note.acp_event_range,
        note.at_ms,
        note.event_kinds,
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
#[cfg(test)]
mod warning_tests {
    use super::exit_warning;

    #[test]
    fn an_exit_code_is_named_as_one() {
        let command = vec!["cmd".to_owned(), "/c".to_owned(), "exit 3".to_owned()];
        assert_eq!(
            exit_warning(&command, 3),
            "warning: [\"cmd\", \"/c\", \"exit 3\"] exited with status 3"
        );
    }

    #[test]
    fn the_control_event_code_is_named_in_words() {
        // A console child killed by a control event reports 0xC000013A, which as a signed
        // integer is this number. Printing the number alone would leave a reader to look
        // it up, and the reader of a warning is usually mid-triage.
        let command = vec!["cmd".to_owned()];
        let warning = exit_warning(&command, -1073741510);
        assert!(warning.contains("console control event"), "{warning}");
    }
}
