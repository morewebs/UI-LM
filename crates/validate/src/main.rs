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
//! # One screen per session, by default
//!
//! The two sides are not aligned sequences. Our recorder frames the screen whenever
//! the child's output has been quiet for the settle gap; the fork dumps the buffer
//! after every draw. A session of forty draws may be thirty frames, and frame 12
//! has no principled relationship to draw 12. What *is* principled is the last
//! screen of each: the recorder frames the final settled screen when the child's
//! output ends, and the fork's last draw is that same screen. That end-of-session
//! pair is the default, and it is one strong data point per session rather than a
//! frame-by-frame alignment. `--ours-seq` and `--theirs-seq` therefore name one
//! screen on each side by its own `seq`, not an index into an aligned sequence.
//!
//! # What is not compared
//!
//! Nothing is masked by default. `Frame::volatile` is a statement our own recorder
//! makes about our own frames, and letting it silence cells here would let the mask
//! hide exactly the disagreement K5 exists to find. The comparison is against the
//! final settled screen, where a spinner has stopped anyway.

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

    /// Which of our frames to compare, by its `seq`. Defaults to the last one.
    #[arg(long, value_name = "N")]
    ours_seq: Option<u32>,

    /// Which of the fork's draws to compare, by its `seq`. Defaults to the last one.
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
        // Said out loud because it is the likeliest cause of a wall of mismatches:
        // the two runs were not on the same size of terminal.
        println!("shapes: the screens are different sizes, so the union of both is compared");
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
/// The last one is the default because it is the pair the two runs are guaranteed
/// to share — see this file's module documentation. A named `seq` is looked up in
/// the run's own numbering, and a number the run does not have is an error listing
/// what it does have, so a typo cannot quietly fall back to the default.
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
