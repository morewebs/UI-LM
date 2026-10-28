# UI-LM

The capture rig for **UI-LM**: a system that reads a terminal application's screen as a cell grid, turns it into a structured description of the page, and renders that description back as a real GUI (or the reverse).

This repository is the first component only — **the instrument, not the model**. It exists so that screens can be recorded deterministically, stored as data, and checked against the application's own draw buffer, before anything is trained on them.

## What is here

| Crate | Responsibility |
|---|---|
| `crates/frames` | The styled cell grid, its run-length codec, the `Frame` model (changed rows, volatile spans, cursor, alt-screen), and the session store (`session.json` + `frames.jsonl`) |
| `crates/capture` | The PTY driver (`portable-pty` + `vt100`), the 150 ms quiescer, the volatile-span classifier, the event tap, and the `ui-lm-capture` binary |
| `crates/validate` | `ui-lm-validate` — compares recorded frames against the application's own draw buffer (the K5 check) |
| `crates/pages` | The first IR: a frame becomes an A2UI `createSurface` message using the `moreweb.terminal/v1` catalogue |

The page slice is deliberately minimal but no longer trivial. It finds blocks — runs of
non-blank rows — splits a transcript at its tool lines, and emits one component per thing it
can honestly name:

- **`Block`** — a run of rows that reads as one thing, with a role, its lines, and the styling
  of each line as per-span attributes (`bold`, `italic`, `underline`, `reverse`, and colours
  as `indexed:N` or `rgb:R,G,B`). Per-span attributes are the gap every surveyed standard has.
- **`ToolCall`** — a run of `♦`-prefixed lines, each with its tool (`Read`, `Edited`), its
  target, a `status`, and for an edit the `+A -R` counts the application prints.
- **`Meter`** — a line reporting a quantity against a limit (`1.3K / 500K │ [Dashboard]`),
  keeping the drawn figures and computing the fraction beside them.
- **`Diff`** — a block carrying both a `+` and a `-` line, with a per-line kind. No screen in
  the corpus has drawn one yet: with auto-approval on, an edit is reported as a `ToolCall`
  line, which is why the detector is tested against the shape rather than a capture.

Every component carries `source: "seen"`, and describing a whole session marks one
`carried` once it has already appeared. There is no `Table` or `Thought` yet, no actions, and
no `guessed` provenance, because nothing has been reconstructed. A component's id is derived
from its own text rather than its position, so it keeps its id when it scrolls. Role detection
reads only a block's first line, so a
turn whose prompt line sits below other text is reported as `text`.

Plus two operator scripts: `scripts/capture-w1.ps1` (a scripted capture session) and `scripts/k5-dump.md` (how to build a throwaway patched fork that dumps its draw buffer, and how to compare).

## Download

Binaries for Windows, Linux and macOS are attached to the [latest
release](https://github.com/morewebs/UI-LM/releases/latest). Download the archive for your
platform, unpack it, and run:

```
ui-lm-capture --help
```

Three programs come in each archive: `ui-lm-capture` records and describes, `ui-lm-serve`
serves a live view, and `ui-lm-validate` compares a recording against an application's own
draw buffer. Since 0.3.1 the trained model travels with them, in `models/page.json`, so a
download can score and describe a screen without the repository. No Rust toolchain, nothing to
install.

## Quickstart

```bash
cargo build --workspace
cargo test --workspace        # 221 tests
```

Record a session of any terminal application:

```bash
ui-lm-capture record --rows 50 --cols 160 --out sessions/example \
  --cwd /path/to/scratch -- -- the-program --its-args
```

Each settled screen becomes one line of `frames.jsonl`: run-length cells with character, foreground, background, bold/italic/underline/reverse and a `wide_tail` flag for full-width glyphs. `session.json` records the command, geometry, the rendering-relevant environment, and where the child ran.

A frame exists only at a **settled** point — 150 ms of quiet. An animating screen produces frames at its pauses, not continuously.

## The model

A trained model turns a screen's text into the page that describes it — blocks, tool calls, meters, reasoning lines, diffs, and the panes of a full-screen application. It is a softmax regression over nineteen features read from the screen itself, trained by `scripts/train_model.py` on recorded screens and run in Rust by `crates/model`.

**Trained on 270 screens of one application, it reproduces every component of 141 frames from software it was never trained on** — `gh`, `git`, `npm`, `pip`, `cargo`, vim, tmux, nano, and screens where the application is failing — against floors between a quarter and seven eighths. Four batches recorded after training have all come back at 100%. The floor is a converter that only splits paragraphs, so a screen it already reproduces exactly proves nothing; `scripts/check-all.ps1` reports which sets can still fail.

```bash
ui-lm-capture page --session sessions/example --model models/page.json   # what the model sees
ui-lm-capture score --session sessions/example --model models/page.json   # model beside the floor
ui-lm-serve --session sessions/live --keys keys.txt --model models/page.json   # the live view, drawn from the model
powershell -File scripts/check-fresh.ps1   # record new material and score it blind
powershell -File scripts/check-all.ps1     # every set, both directions, provenance
```

## See it live, and type back

Two processes: one records, one serves the recording and forwards what you type.

```bash
ui-lm-capture record --out sessions/live --control keys.txt -- --some-program
ui-lm-serve --session sessions/live --keys keys.txt --port 8787
# open http://127.0.0.1:8787/
```

The browser shows the program's screens as they settle, and the box at the bottom types into
it — the text goes to `keys.txt`, the recorder forwards it to the program's terminal. It
listens on this machine only: no account, no TLS, nothing reachable from elsewhere. A screen
appears when the program settles, so a spinner mid-animation is not shown; it never was a
frame.

Type into a program while it runs, from outside the recorder:

```bash
ui-lm-capture record --out sessions/live --control keys.txt -- --some-program
printf 'hello\r' >> keys.txt      # the program receives a typed line
```

The channel is a file rather than a socket on purpose: no port to open, nothing reachable
from another machine, and the same behaviour everywhere. The recorder forwards only what is
new since it last looked, so a long session does not replay its history.

Turn a recording into training pairs — the screen's text, and the page that describes it:

```bash
ui-lm-capture dataset --session sessions/example --out pairs.jsonl
```

One JSON object per line with `input` (what the screen said, row by row) and `target` (the
page, as JSON). Frames with nothing on them are left out. This is the shape a model would
learn: convert one, get the other.

Draw a recording as a page you can open in a browser:

```bash
ui-lm-capture render --session sessions/example --all --out example.html
```

That is the whole loop in one command: a program was observed, its screens were described, and the description was drawn by something that never saw the terminal. [docs/demo/fixture.html](docs/demo/fixture.html) is a rendering of the checked-in recording, so the output can be looked at without building anything.

Read back what a corpus holds:

```bash
ui-lm-capture summarize -- sessions/
```

```
1 session(s), 20 frame(s), 18 distinct screen(s)
20.1 s of recorded wall clock
masked cells: 36; frames naming an event range: 16; frames carrying kinds: 5
events labelled by the application:
  agent_message_chunk: 1
  hook_execution: 5
  tool_call: 2
  tool_call_update: 4
  turn_completed: 1
```

Killed captures are included: their frames are real screens, so leaving them out would understate the corpus.

## What is proven, and what is not

**Proven.** The rig's VT100 emulator reproduces the application's own draw buffer exactly: 8000/8000 cells, styles included, on every like-for-like pair measured across three scenarios, one containing a styled full-width glyph. See [the K5 measurement](docs/reports/2026-10-27-k5-measurement.md). A real capture of 35 sessions / 526 frames / 436 distinct screens against Grok Build is described in [the W1 capture report](docs/reports/2026-10-27-w1-capture.md).

**Proven since.** A screen's text is described as a page — blocks, tool calls, meters, reasoning lines, diffs and panes — and the page is drawn back: HTML for a browser, and terminal text that returns **every line of content** on every recording measured, split screens included. A page carries the styling the recorder read (6,303 spans across the training batch) and says honestly whether each component was read from the screen or carried from an earlier one (`ui-lm-capture provenance`: 967 components, 0 contradictions). The model fills that page, and `ui-lm-capture self-check` separates what it *judged* from what the code *built*, so a wrong page can be attributed rather than guessed at.

**Not proven, and not claimed:**

- **The training material is one agent's transcripts.** Every number rests on 270 screens of Grok Build, because only agents draw the components this catalogue is for: seven other programs recorded for training produced eleven screens and not one meter, tool call, diff or thought — twenty-six blocks and nothing else. The model reads those programs correctly; there was nothing in them to train on.
- **The evidence is this machine's.** Every evaluation set was recorded here, mostly chosen by the person building the model. Four clean fresh batches is the strongest claim the project can make, and it is a modest one.
- **Structure comes from applications that render state.** Agent transcripts and full-screen applications draw diffs, meters, tool calls and panes; compiler output and build logs draw words. On plain text the floor already reproduces the screen exactly, so nothing is measured there and those frames are not counted as evidence.
- **Provenance is honest but a live view is not continuous.** A frame exists at a settled screen, so a fast-moving program yields fewer moments than it drew.
- **Volatile-span masking needs the program's event stream.** The classifier is
  implemented and tested, and the recorder follows the application's own ACP event stream
  (`--events`, default `~/.grok/sessions`). Frames recorded before that work carry
  `volatile: []` and should be re-captured before anything is trained on them.
- **Alt-screen frames depend on the pty host, not the emulator.** Whether a child's
  `ESC[?1049h` reaches the rig is decided by the host: one Windows conhost consumed the
  switch and re-rendered a delta, while a GitHub `windows-latest` runner forwarded it. Unix
  ptys hand the bytes over unchanged.
- **An unnumbered tab strip is still plain text.** An editor that numbers its tabs is read as
  tabs; one that does not is read as a line of words, because two bare file names are not
  distinguishable from two words.
- **Cost is not recorded.** The TUI stream carries no usage events, so `SessionRecord.cost` is `null`.

## Design

The design and the decisions behind it are in the repo:

- [Design spec](docs/superpowers/specs/2026-10-27-ui-lm-design.md) — intent, goals and anti-goals, constraints with sources, 22 decisions with the alternatives each rejected
- [Capture rig plan](docs/superpowers/plans/2026-10-27-ui-lm-capture-rig.md) — the task-by-task implementation plan this code was built from
- [Glossary](GLOSSARY.md) — frame, page, surface, provenance, patch, and the words to avoid
- [ADR 0001: the cell grid is the model's input, not the event stream](docs/adr/0001-cell-grid-as-model-input.md)
- [ADR 0002: A2UI's envelope, our catalogue](docs/adr/0002-a2ui-envelope-custom-catalogue.md)
- [The catalogue reference](docs/catalogue/moreweb.terminal-v1.md) — every component, what each guarantees, and which have been seen on a recorded screen

## Picking this up

[HANDOFF.md](HANDOFF.md) is the short version: where everything lives, how to verify it, what is
proven and what is not, the fragile spots and why they are written they way they are, and the
three things worth doing next.

## Licence

Apache-2.0. See [LICENSE](LICENSE).
