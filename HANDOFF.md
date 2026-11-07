# Handoff

> **The short version:** `docs/reports/2026-10-29-month-in-review.md` — what this is, what it
> proves, what it cannot, and how to check it in one command.

Written at the end of a month's autonomous work. Everything here was measured; where it wasn't,
the gap is named.

## What this is

A terminal screen is read into a **page** — a description of what was on screen: blocks of text,
tool calls, meters, reasoning lines, diffs, tables, floating windows, and the panes of a full-screen
application — and the
page is drawn again somewhere else: HTML for a browser, terminal text for a terminal, or live as
the screen changes. A small trained model decides what each row *is*; the code that watched the
screen decides how it *looks* and how a component is shaped.

## Where things are

| | |
|---|---|
| `crates/capture` | records a program on a pty, and the CLI: `record summarize audit page score tui self-check fields disagree provenance baseline dataset render frames compare version read` — `ui-lm-capture read --html <FILE>` gives the screen an HTML document describes |
| `crates/frames` | the recording format — run-length cells with character and style |
| `crates/pages` | the reader: screen → page, the A2UI envelope, the catalogue, `compare` |
| `crates/model` | nineteen features, the weights, inference in Rust |
| `crates/render` | page → HTML, page → terminal text |
| `crates/serve` | the live view: serves pages, forwards keystrokes back to the program |
| `crates/validate` | compares a recording against an application's own draw buffer |
| `scripts/train_model.py` | training; writes `models/page.json` |
| `scripts/check-all.ps1` | every evaluation set, both directions, provenance — one exit code |
| `scripts/check-fresh.ps1` | records new material and scores it blind; the only check that finds new defects, and it records one screen for each kind that needs no agent so a batch cannot lose them silently |
| `scripts/retrain.ps1` | record → export → train → evaluate in one run |
| `docs/reports/2026-10-28-model.md` | the full record: every number, every attempt, every failed hypothesis |
| `docs/implementing.md` | **the guide for a runtime this project did not write**: what a page gives you, what a runtime owes it, and the three tests that say whether you got it right |
| `docs/catalogue/moreweb.terminal-v1.schema.json` | the format, decided: derived from 1,639 pages, validated by `check-schema` |
| `docs/conformance/` | eight stored pages and the screen each must draw, held by `check-conformance` |

## How to verify anything

```powershell
powershell -File scripts/check-everything.ps1    # every check below, one table, about half an hour
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
powershell -File scripts/check-scrub.ps1        # a shared pair keeps its catalogue and loses its account
powershell -File scripts/check-corrupt.ps1       # damaged recordings are skipped, not fatal
powershell -File scripts/check-readme.ps1        # every command and script the documents name exists
powershell -File scripts/check-docs.ps1          # the documents' numbers match the tree

# The seven below, in order. The audit is the evidence; nothing below is a claim
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
| a screen's text becomes a page holding blocks, tool calls, meters, thoughts, diffs, frames, tables and panes | **seven kinds**, 779 frames assembled at 100% given the reader's own answers |
| a page draws back every line of content | **0 sessions below 100%**, split screens and resized dividers included, on both reader and model pages |
| the page carries the screen's styling | 5,957 components compared field by field, **0 differing** |
| provenance is honest | 12,595 components, 10,504 carried, **0 contradictions** |
| the reader and the label path agree | every evaluation set at 100%, and every hard case on score, round trip and shape |
| the model generalises off its training application | trained on Grok alone; **591 frames of Freebuff at 100%** — Block 4652, ToolCall 158, Thought 385, Diff 54 |
| the download works | **v0.65.0** verified by downloading, unzipping, and running `version` and `score` on a real session |
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

## What is next, and what would reopen the closed questions

Seven items from earlier handoffs are done, and saying so is cheaper than a successor rediscovering it: `Frame` exists
(vim's popup, two whiptail dialogs) and now admits a third application - tmux's untitled popup, by centring (ADR 0006's
third shape; 6 centred boxes in 646, all floating windows); a record names its program's exit (`SessionRecord::exit`);
`Meter` has a producer that is not an agent (`git clone --progress`); interaction has a check; a word grid with no
underline is a `Table` (ADR 0009, seven screens across five applications, no prose, and a listing whose last row
stops short is still a grid); and the reader's answer to a
framework-shaped page is measured and held by a fixture.

Two questions are closed **by decision**, and each says what would reopen it:

1. **A record names the program a wrapper runs: no.** `wsl.exe -e sh /tmp/ui-lm/wt.sh` hides a whiptail dialog, and the
   record can only measure the command it spawned. A `label` field would put an assertion beside the measurements the
   rest of the record is made of; the producer count stays a lower bound, printed with the wrapper (`wsl->sh`). Reopen
   if a consumer needs the name badly enough to accept a claimed one.
2. **Whether losing side-by-side layout matters: no consumer has asked.** A framework-shaped page reads as its blocks
   in document order and loses the CSS layout, and a page cannot carry a layout without a layout engine. Reopen with a
   consumer that needs columns.

Measured since, and recorded where a successor will look: the pane model is **two panes, side by side**, so a third
pane reads as two and a stacked split reads as blocks in document order - both content-exact, both with reopening
conditions (ADR 0004's addendum); a page's line is a row of **cells**, so a wide glyph keeps a space for its second cell
and a combining mark is dropped - stated in the catalogue, with tests in `crates/pages` and `crates/frames`.

What remains is material and release: `check-fresh` batches, another application for a thin kind, and the next release -
all of which need either the machine or a decision that is not this document's to make.

## What was the second item, and is now done

**A corpus from a second agent** — Freebuff, an npm TUI, installed and recorded eleven times: 580 frames
across reading, writing, diffing (including a rename), searching, a full fix cycle, and two refusals. The model
was trained on Grok alone and reproduces **every component at 100%**. `check-agents` holds it, and the
decisions that came out of it are in `docs/reports/2026-10-28-model.md`: a bullet marks a tool row, a reasoning
heading ends after its word, a border ends in a corner, and the scrollbar column belongs to the terminal rather
than to the application.

## Releasing

`docs/release.md` is the procedure, and the order in it matters: **pages before tag**. `check-docs` compares a
front page's version with the newest tag, so tagging first leaves `main` naming the previous release until
somebody gets round to the documents — which is what happened for twenty-four releases.

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
## Where it stands now

**31 checks, about half an hour, every axis green**, **23 hard cases** at 100% on score, round trip and
shape, and **v0.65.0** verified by download and by asking the binary what it was built from (`version` prints the
commit and a digest of the working tree).

The corpus is **124 recordings, 1685 frames** per path, recorded from **12 distinct commands** — derived from the
`command` field of every recording, which is the only definition that admits checking. Among them: Grok (the training
application), Freebuff (a second agent CLI), `whiptail` (the only program here that draws a floating dialog), and
`man`, `less`, `df`, `vim`, `top`, `git`, `cmd` and PowerShell — programs that are not agents at all.

**Every one of those was recorded on this machine.** A GitHub runner records three shapes of its own — a listing, a
process monitor and a whiptail dialog — so the reader is measured away from this host, but those recordings live in CI
rather than here, and the numbers above say nothing about them.

### Attempts that were made and abandoned, so they are not repeated

Each of these was tried, measured, and reverted. The reason matters more than the outcome.

| attempt | why it failed |
|---|---|
| widen the pane rule so a dialog would not mask a split | the obstacle was not the box guard but the single-pane decision |
| give panes their styling (reader only) | worked — 25 of 26 lines per pane — but the model pages had none |
| drive `tmux` with keystrokes to build a layout | `tmux send-keys` never reaches an agent under it; the recorder's own key script does |
| add the pane field to `Column` | 11 construction sites; moved to `Surface` (5), which landed |
| narrow `Diff` to require a hunk header | **34 of the corpus's 61 diffs have no header** — Freebuff shows edits as bare `-`/`+` |
| read a table from column alignment | re-measured in round 193: the rule matches vim's help rows and scores **0 on `df`** — ADR 0007 is Rejected, and `scripts/measure-tables.py` reproduces it |
| move the wait above the record write | five attempts; the status is one `if let` away, and `record_exit` stamps it afterwards |

**The pattern in the failures is symmetry and evidence:** every change that made one side of a mapping richer
before the other could follow was reverted; every change that added an inert field first landed; and every kind
refused was refused by counting something.

### Two questions this section used to ask, and their answers

1. **A page that is not text.** A framework-shaped page - layout divs, CSS classes, no semantic tags - reads back as its
   text in document order: inline spans join on a line, each block element is its own line, and no rows are invented
   from CSS. That is the answer to "what does it say when it recognises nothing", and `check-html-to-tui` holds it with
   a ten-line fixture. The cost is named: side-by-side layout becomes a column.
2. **Producers are counted, and the count is a lower bound.** `scripts/measure-producers.py` reads every session's
   `command` and, where the wrapper names one, the inner program after `wsl.exe -e`. `Diff` has four commands (git,
   Freebuff under `cmd.exe`, a Python script, a shell script); `Frame` has two applications - vim's floating popup and
   whiptail's dialogs - but one command field (`wsl.exe`), because `sh /tmp/ui-lm/wt.sh` hides the program under the
   shell. The limit is named rather than hidden: a producer count from the record is a lower bound.

**The provenance contradiction is fixed.** `pages()` decides `carried` from the memory as it was **before** the frame,
not from the set the loop is filling: two identical blocks in one frame are both `seen`, because neither was on screen
earlier. The corpus measures **12,595 components, 10,504 carried, 0 contradictions**, and `check-corpus` now pins zero.
The two earlier attempts failed because they also lost the restore step; the smallest test of the shape is
`two_identical_blocks_in_one_frame_are_both_seen` in `crates/pages/src/lib.rs`.

## Two open threads, each with its next step

1. **`Table` shipped, and both signals are measured.** Values catch numeric columns; a dash underline catches a word
   table (`cases/hard/pwsh-word-table`, required). A table inside a pane is `cases/hard/pane-with-tables`. The heading and
   the dash row are cell rows too - skipping them made the audit fail and left four lines of `pwsh-table` out of the
   document. v0.45.0 cut each cell at the end of the dashes (`hosts` stored as `host`); the cell now runs to the next
   column, which is what the values rule already did. **Next step:** a word table the application does not underline,
   which neither signal can see, and which the corpus does not hold.
2. **The reader's silence is measured, and the answer is "the document's own blocks, in order".** A page built from
   layout divs and CSS - the shape a framework emits - reads back as its text in document order: inline spans join on
   a line, each block element is its own line. No rows are invented from CSS, and nothing is dropped.
   `check-html-to-tui` holds a framework-shaped fixture and its ten lines, so the answer cannot change silently. A
   real page from a sibling project on this machine (a server-rendered template, not a live app) read as 21 lines,
   template syntax included. **Next step:** whether losing side-by-side layout is acceptable to a consumer, and a
   capture of an application's rendered DOM rather than a template.

**None of these is a defect in the code that ships.** They are the frontiers: what the reader has not been shown,
what the catalogue has refused, and what the vocabulary cannot yet be asked twice.