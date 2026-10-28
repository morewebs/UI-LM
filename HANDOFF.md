# Handoff

Written at the end of a month's autonomous work. Everything here was measured; where it wasn't,
the gap is named.

## What this is

A terminal screen is read into a **page** — a description of what was on screen: blocks of text,
tool calls, meters, reasoning lines, diffs, and the panes of a full-screen application — and the
page is drawn again somewhere else: HTML for a browser, terminal text for a terminal, or live as
the screen changes. A small trained model decides what each row *is*; the code that watched the
screen decides how it *looks* and how a component is shaped.

## Where things are

| | |
|---|---|
| `crates/capture` | records a program on a pty, and the CLI: `record summarize audit page score tui self-check provenance baseline dataset render compare` |
| `crates/frames` | the recording format — run-length cells with character and style |
| `crates/pages` | the reader: screen → page, the A2UI envelope, the catalogue, `compare` |
| `crates/model` | nineteen features, the weights, inference in Rust |
| `crates/render` | page → HTML, page → terminal text |
| `crates/serve` | the live view: serves pages, forwards keystrokes back to the program |
| `crates/validate` | compares a recording against an application's own draw buffer |
| `scripts/train_model.py` | training; writes `models/page.json` |
| `scripts/check-all.ps1` | every evaluation set, both directions, provenance — one exit code |
| `scripts/check-fresh.ps1` | records new material and scores it blind; the only check that finds new defects |
| `scripts/retrain.ps1` | record → export → train → evaluate in one run |
| `docs/reports/2026-10-28-model.md` | the full record: every number, every attempt, every failed hypothesis |

## How to verify anything

```powershell
powershell -File scripts/check-everything.ps1    # every check below, one table, ~6 minutes
powershell -File scripts/check-all.ps1           # every set, both directions, provenance
powershell -File scripts/check-fresh.ps1         # record new material and score it blind
powershell -File scripts/check-interaction.ps1   # typing into the live view reaches the program
powershell -File scripts/check-race.ps1          # reads during a redraw, and a half-written frame
powershell -File scripts/check-live-page.ps1     # the live page moves, a finished one holds still
powershell -File scripts/check-render-scale.ps1  # exporting a session stays linear per page
powershell -File scripts/check-release.ps1       # the published download does what the README says
powershell -File scripts/check-validate.ps1      # the validator agrees on a match, disagrees on a difference
powershell -File scripts/check-hard.ps1         # the two screens that drove the pane container, both at 100%
powershell -File scripts/check-corpus.ps1       # every recording here reads without an error
powershell -File scripts/check-html.ps1         # every word the page holds reaches the browser document
powershell -File scripts/check-audit.ps1        # every page row matches the screen it came from
powershell -File scripts/check-sizes.ps1        # the smallest and largest screens round trip every line

# All seven, in order, about six minutes. The audit is the evidence; nothing below is a claim
# that has not been through it.
foreach ($s in "check-all","check-interaction","check-race","check-live-page","check-render-scale","check-release","check-fresh") {
    powershell -NoProfile -File "scripts/$s.ps1"
}
cargo test --workspace                           # 221 tests
```

`check-all` fails if any set drops below 100%, if the label path stops rebuilding the reader's
pages, if provenance contradicts itself, or if a page stops drawing back every line.

## What is proven

| claim | evidence |
|---|---|
| a screen's text becomes a page holding blocks, tool calls, meters, thoughts, diffs, panes | 42 evaluation sets, 100% |
| a page draws back every line of content | 1,986/1,986 lines; split screens included; **the same on model-built pages** |
| the page carries the screen's styling | 6,303 spans on the training batch; reaches HTML, terminal and the live view |
| provenance is honest | 967 components, 0 contradictions |
| the model generalises off its training application | trained on one application, 100% on 141 frames from 13 other programs |
| the download works | **v0.5.0** verified by downloading, unzipping and running `fields` and `tui --model` |

## What is not proven

- **One machine, one recorder's judgement.** Every evaluation set was recorded here and mostly
  chosen by the person building the model. Four consecutive clean fresh batches is the strongest
  claim available, and it is modest.
- **The corpus cannot be broadened the obvious way.** Seven non-agent programs recorded for
  training produced eleven screens and not one structured component: only software that renders
  *state* draws the things this catalogue describes.
- **Plain text measures nothing.** A paragraph splitter reproduces a build log exactly, so those
  frames are not counted as evidence.
- **An unnumbered tab strip is plain text.** An editor that numbers its tabs is read as tabs; one
  that does not is indistinguishable from a line of words.
- **Live is not continuous.** A frame exists at a settled screen; a fast program yields fewer
  moments than it drew.

## Fragile spots, and why they are written this way

- **Assembly guards are deliberately narrow.** `crates/pages/src/labeled.rs` demotes labels the
  text cannot support (a lone `+ New Line` is not a diff, `> /import-memory` is not a meter, a
  `✓` table row is not a thought). Every one was added after a screen failed, and every broader
  version of one cost points elsewhere. `crates/pages/tests/guards.rs` holds 14 cases, each with
  a positive and a negative.
- **The pane rule refuses boxed screens.** It splits a screen into panes only when the screen is
  one block, has no box borders, and has a column of `|` down most rows. Widening it tore every
  panel this project already described in half.
- **A label that decides grouping must be right before grouping.** Correcting a label after
  `runs()` cannot put a split block back together. Three attempts to merge afterwards all failed.
- **The tool verbs are a list.** Adding `Explored`, `Search`, `Found`, `Grepped` moved the page
  score further than four recording batches did. When a new tool appears, the model cannot see it
  until its verb is in `crates/model/src/features.rs` and `scripts/train_model.py` together.

## The three things I would do next

1. **A third full-screen application with a floating window or a dialog.** vim and tmux both put
   panes in a plain rectangle; the design's trigger for a `Frame` component has not fired yet.
2. **A corpus from a second agent.** The training material is one agent's transcripts because only
   agents draw structure — a second agent's transcripts are the only way to test whether those
   shapes are general or Grok's.
3. **Interaction.** Nothing here tests a screen mid-redraw or a keystroke arriving during a
   redraw; `serve` forwards input but no test asserts what happens when it does.

## Conventions this project runs on

- **Never commit on a failed gate, and make the gate animate it.** `scripts/gate.ps1` runs the three
  checks and refuses to commit unless all pass (`-Commit "message"`). In round 101 a commit went
  through while clippy and tests were failing because the discipline lived in my attention; it now
  lives in the script.
- **Measure every claim; record every disproved hypothesis.** The report names the attempts that
  failed and what they cost, because that is the only reason the next person won't repeat them.
- **A check that cannot see its material must fail, not pass.** Three separate defects this month
  were commands silently ignoring a flag; one was a sweep that reported a pass while skipping two
  of its four checks.
- **No vocabulary from one screen.** Components and roles are added with three screens of
  evidence, or not at all.
## Where it stands at the end of the month

Seven scripts, about six minutes, every axis green: five evaluation axes (kind, text, styling,
provenance, round trip on both reader and model pages), 683 components with 0 differences and 0
contradictions, five consecutive clean fresh batches, a three-platform release verified by download.

### Attempts that were made and abandoned, so they are not repeated

Each of these was tried, measured, and reverted. The reason matters more than the outcome.

| attempt | why it failed |
|---|---|
| widen the pane rule so a dialog would not mask a split | the obstacle was not the box guard but the single-block precondition; the change did nothing |
| give panes their styling (reader only) | worked — 25 of 26 lines per pane — but the model pages have no panes in five sessions, so `fields` went from 0 to 12 differing |
| give panes styling on both sides at once | the label path's pane condition is the blocker; changing one side first makes every axis worse |
| drive `tmux` with keystrokes to build a specific layout | the prefix and its command need **separate writes**; in one write the text lands in the shell |
| add the pane field to `Column` | 11 construction sites; moved to `Surface` (5), which landed |

**The pattern in the failures is order and symmetry:** every change that made one side of a mapping
richer before the other could follow was reverted, and every change that added an inert field first
landed.

### The one thing worth doing next

Make the label path split panes the same way the reader does — one condition in
`crates/pages/src/labeled.rs` — and change both sides in a single commit. That is the whole of
ADR 0004's first step, and three rounds of evidence say it cannot be done half at a time.