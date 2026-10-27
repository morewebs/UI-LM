# UI-LM

The capture rig for **UI-LM**: a system that reads a terminal application's screen as a cell grid, turns it into a structured description of the page, and renders that description back as a real GUI (or the reverse).

This repository is the first component only — **the instrument, not the model**. It exists so that screens can be recorded deterministically, stored as data, and checked against the application's own draw buffer, before anything is trained on them.

## What is here

| Crate | Responsibility |
|---|---|
| `crates/frames` | The styled cell grid, its run-length codec, the `Frame` model (changed rows, volatile spans, cursor, alt-screen), and the session store (`session.json` + `frames.jsonl`) |
| `crates/capture` | The PTY driver (`portable-pty` + `vt100`), the 150 ms quiescer, the volatile-span classifier, and the `ui-lm-capture` binary |
| `crates/validate` | `ui-lm-validate` — compares recorded frames against the application's own draw buffer (the K5 check) |

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

## What is proven, and what is not

**Proven.** The rig's VT100 emulator reproduces the application's own draw buffer exactly: 8000/8000 cells, styles included, on every like-for-like pair measured across three scenarios, one containing a styled full-width glyph. See [the K5 measurement](docs/reports/2026-10-27-k5-measurement.md). A real capture of 35 sessions / 526 frames / 436 distinct screens against Grok Build is described in [the W1 capture report](docs/reports/2026-10-27-w1-capture.md).

**Not proven, and not claimed:**

- **Volatile-span masking is inert.** The classifier is implemented and tested, but no event stream is wired into the recorder, so every recorded frame carries `volatile: []`. Spinner and clock changes are recorded as content. Any dataset built from these frames needs the mask wired first.
- **Alt-screen frames cannot be recorded on Windows.** ConPTY consumes `ESC[?1049h` and re-renders a delta, so the switch never reaches the emulator. Real alt-screen data needs a passthrough PTY or a Unix capture.
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
