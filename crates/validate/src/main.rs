//! `ui-lm-validate` — the K5 verifier: compares captured frames against the
//! application's own draw buffer.
//!
//! The rig records what a VT100 emulator says a screen is. The application's draw
//! buffer says what the application put there. If the two disagree, every frame,
//! eval number and training pair downstream describes a screen nobody ever saw, so
//! this binary exists to make the disagreement visible: `compare` reads a session
//! the recorder wrote and a dump the patched fork wrote, compares one screen from
//! each, prints what it found, and exits non-zero when anything differs.
//!
//! # The screens to compare are named, not assumed
//!
//! The two sides are not aligned sequences. Our recorder frames the screen whenever
//! the child's output has been quiet for the settle gap; the fork dumps the buffer
//! after every draw. A session of forty draws may be thirty frames, and frame 12
//! has no principled relationship to draw 12. What the two sides *do* share is a
//! screen — the same screen — and naming it on each side is the operator's job:
//! `--ours-seq` and `--theirs-seq` name one screen by its own `seq`, not an index
//! into an aligned sequence.
//!
//! Defaulting both to the last screen is **not** a safe pairing for this
//! application and must not be reported as a K5 result. Measured on 2026-10-27: on
//! its way out the application leaves the alternate screen and repaints its
//! transcript in default style, so our last frame is a post-exit repaint that no
//! draw buffer ever held, and last-vs-last reported 8000 of 8000 mismatches on
//! screens that agree cell for cell. `scripts/k5-dump.md` §5 has the procedure and
//! the like-for-like pairs.
//!
//! # A shape difference is a hard failure
//!
//! The two selected screens must be the same `rows x cols`. When they are not, the
//! command prints both shapes and exits non-zero without comparing anything. The
//! union of two differently-shaped screens reports every cell outside the smaller
//! one as a mismatch, and that count cannot be told apart from real emulator drift —
//! so it would dress a rig error up as the finding K5 exists to produce.
//!
//! # What is not compared
//!
//! Nothing is masked by default. `Frame::volatile` is a statement our own recorder
//! makes about our own frames, and letting it silence cells here would let the mask
//! hide exactly the disagreement K5 exists to find.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};

use ui_lm_frames::{read_frames, Frame};

mod compare;
mod dump;

use compare::compare_frames;

/// The command line: `ui-lm-validate compare …`.
#[derive(Debug, Parser)]
#[command(
    name = "ui-lm-validate",
    version,
    about = "Compares recorded frames against the application's own draw buffer"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compare a recorded session's screen against a dumped draw buffer.
    Compare(Compare),
}

#[derive(Debug, Args)]
struct Compare {
    /// The session directory the recorder wrote: `session.json` and `frames.jsonl`.
    ///
    /// Read with `read_frames`, so it has to be a finished session. A capture
    /// killed before it could write its record has frames but no record, and the
    /// error says so rather than silently comparing an earlier screen.
    #[arg(long, value_name = "DIR")]
    ours: PathBuf,

    /// The draw-buffer dump the patched fork wrote: one JSON object per draw.
    #[arg(long, value_name = "FILE")]
    theirs: PathBuf,

    /// Which of our frames to compare, by its `seq`. Defaults to the last one,
    /// which is only a like-for-like screen if the application does not repaint
    /// after its exit sequence — this one does, so name the pair.
    #[arg(long, value_name = "N")]
    ours_seq: Option<u32>,

    /// Which of the fork's draws to compare, by its `seq`. Defaults to the last
    /// one. Name it in a K5 run: see `scripts/k5-dump.md` §5.
    #[arg(long, value_name = "M")]
    theirs_seq: Option<u32>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("ui-lm-validate: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<ExitCode> {
    match command {
        Command::Compare(args) => run_compare(args),
    }
}

/// Compares one screen from a session against one screen from a dump.
fn run_compare(args: Compare) -> Result<ExitCode> {
    let (record, frames) = read_frames(&args.ours)
        .with_context(|| format!("failed to read the session in {}", args.ours.display()))?;
    let dumps = dump::read_dump(&args.theirs)
        .with_context(|| format!("failed to read the dump {}", args.theirs.display()))?;

    let (ours_index, ours) =
        select(&frames, args.ours_seq, "frame").context("in the recorded session")?;
    let (theirs_index, theirs) =
        select(&dumps, args.theirs_seq, "draw").context("in the draw-buffer dump")?;

    println!(
        "session: {} started {}, recorded at {}x{}",
        record.command, record.started_at, record.rows, record.cols
    );
    println!(
        "ours:   {} — frame {} of {} (seq {}), {}x{}",
        args.ours.display(),
        ours_index + 1,
        frames.len(),
        ours.seq,
        ours.rows,
        ours.cols
    );
    println!(
        "theirs: {} — draw {} of {} (seq {}), {}x{}",
        args.theirs.display(),
        theirs_index + 1,
        dumps.len(),
        theirs.seq,
        theirs.rows,
        theirs.cols
    );
    if (ours.rows, ours.cols) != (theirs.rows, theirs.cols) {
        // A hard failure, not a comparison. The union of two differently-shaped
        // screens reports every cell outside the smaller one as a mismatch, and that
        // number is numerically indistinguishable from real emulator drift — the one
        // thing K5 exists to detect. Stop, and say which side is which size, so the
        // operator reads this as "our frames are the wrong size" rather than "we
        // disagree on ten cells".
        return Err(anyhow!(
            "the screens are different shapes: ours is {}x{} and theirs is {}x{}, \
             so there is no cell-by-cell comparison to make — the two runs were not \
             recorded on the same size of terminal",
            ours.rows,
            ours.cols,
            theirs.rows,
            theirs.cols
        ));
    }

    let comparison = compare_frames(ours, theirs, &[]);
    println!("total cells: {}", comparison.total_cells);
    println!("mismatched:  {}", comparison.mismatched);
    match comparison.first_mismatch {
        Some((row, col, mine, other)) if mine == other => {
            // The characters agree, so the disagreement is styling: without saying
            // so, the line would read as a contradiction.
            println!("first mismatch: {row},{col}: ours={mine:?} theirs={other:?}");
            println!("  (the same character; its style differs)");
        }
        Some((row, col, mine, other)) => {
            println!("first mismatch: {row},{col}: ours={mine:?} theirs={other:?}");
        }
        None => {}
    }

    if comparison.mismatched == 0 {
        println!(
            "K5: the emulator's screen and the draw buffer agree on all {} cell(s)",
            comparison.total_cells
        );
        Ok(ExitCode::SUCCESS)
    } else {
        println!(
            "K5: the emulator's screen and the draw buffer disagree on {} of {} cell(s)",
            comparison.mismatched, comparison.total_cells
        );
        Ok(ExitCode::FAILURE)
    }
}

/// Picks the screen to compare: the one whose `seq` is `seq`, or the last one.
///
/// The last one is the fallback, and it is only the right screen when the
/// application's final recorded frame is one it also drew as a buffer — false for
/// this application, whose last frame is a post-exit repaint (see this file's
/// module documentation). A named `seq` is looked up in the run's own numbering,
/// and a number the run does not have is an error listing what it does have, so a
/// typo cannot quietly fall back to the default.
fn select<'a>(screens: &'a [Frame], seq: Option<u32>, what: &str) -> Result<(usize, &'a Frame)> {
    let last = screens
        .len()
        .checked_sub(1)
        .ok_or_else(|| anyhow!("no {what} to compare"))?;
    let index = match seq {
        None => last,
        Some(seq) => screens
            .iter()
            .position(|screen| screen.seq == seq)
            .ok_or_else(|| {
                anyhow!(
                    "no {what} with seq {seq}; this run holds {} {what}(s), seq {} to {}",
                    screens.len(),
                    screens[0].seq,
                    screens[last].seq
                )
            })?,
    };
    Ok((index, &screens[index]))
}
