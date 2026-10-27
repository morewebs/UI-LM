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

The page slice is deliberately minimal. It finds blocks — runs of non-blank rows — and emits
one `Block` per block with its row span, its lines, a role, and the styling of each line as
per-span attributes (`bold`, `italic`, `underline`, `reverse`, and colours as `indexed:N` or
`rgb:R,G,B`), every component marked `source: "seen"`. Per-span attributes are the gap every
surveyed standard has and the thing a `Diff` will be built from. There is no `Diff`, `Table`,
`Meter`, `ToolCall` or `Thought` yet, no actions, and no `guessed` provenance, because
nothing has been reconstructed. A block's id is derived from its own text rather than its
position, so a block that scrolls keeps its id, and describing a whole session marks a block
`carried` once it has already appeared. Role detection reads only a block's first line, so a
turn whose prompt line sits below other text is reported as `text`.

Plus two operator scripts: `scripts/capture-w1.ps1` (a scripted capture session) and `scripts/k5-dump.md` (how to build a throwaway patched fork that dumps its draw buffer, and how to compare).

## Quickstart

```bash
cargo build --workspace
cargo test --workspace        # 102 tests
```

Record a session of any terminal application:

```bash
ui-lm-capture record --rows 50 --cols 160 --out sessions/example \
  --cwd /path/to/scratch -- -- the-program --its-args
```

Each settled screen becomes one line of `frames.jsonl`: run-length cells with character, foreground, background, bold/italic/underline/reverse and a `wide_tail` flag for full-width glyphs. `session.json` records the command, geometry, the rendering-relevant environment, and where the child ran.

A frame exists only at a **settled** point — 150 ms of quiet. An animating screen produces frames at its pauses, not continuously.

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

**Not proven, and not claimed:**

- **Volatile-span masking needs the program's event stream.** The classifier is
  implemented and tested, and the recorder now follows the application's own ACP
  event stream (`--events`, default `~/.grok/sessions`) so it can tell a spinner
  tick from a reported change. On a real recording the mask named exactly the
  status-line cells whose spinner and clock moved while the event stream did not
  grow. Where no event stream can be found the mask stays empty on purpose: an
  unknown event is never assumed absent. See [the event tap
  note](docs/reports/2026-10-27-event-tap.md). Frames recorded before that work
  carry `volatile: []` and should be re-captured before anything is trained on
  them.
- **Alt-screen frames depend on the pty host, not the emulator.** Whether a child's
  `ESC[?1049h` reaches the rig is decided by the host: one Windows conhost consumed
  the switch and re-rendered a delta, while a GitHub `windows-latest` runner
  forwarded it. Unix ptys hand the bytes over unchanged. `portable-pty` 0.9 knows
  the flag that would make this deterministic (`PSEUDOCONSOLE_PASSTHROUGH_MODE`) and
  does not use it.
- **No IR, no model, no runtime.** The page representation, the GUI renderer and the model are later components.
- **Cost is not recorded.** The TUI stream carries no usage events, so `SessionRecord.cost` is `null`.

## Design

The design and the decisions behind it are in the repo:

- [Design spec](docs/superpowers/specs/2026-10-27-ui-lm-design.md) — intent, goals and anti-goals, constraints with sources, 22 decisions with the alternatives each rejected
- [Capture rig plan](docs/superpowers/plans/2026-10-27-ui-lm-capture-rig.md) — the task-by-task implementation plan this code was built from
- [Glossary](GLOSSARY.md) — frame, page, surface, provenance, patch, and the words to avoid
- [ADR 0001: the cell grid is the model's input, not the event stream](docs/adr/0001-cell-grid-as-model-input.md)
- [ADR 0002: A2UI's envelope, our catalogue](docs/adr/0002-a2ui-envelope-custom-catalogue.md)

## Licence

Apache-2.0. See [LICENSE](LICENSE).
