# UI-LM

A **model** that turns a terminal screen into a structured description of the page, and a runtime that draws that
description somewhere else — a browser, or a terminal again. Not a screenshot, not a web view of a terminal: a page of
named components that anything can render.

The model reads the screen's cells, decides what each row *is*, and emits the page. The code around it decides how a
component is *shaped*. That split is the whole design: what a thing is, is learned; how it looks, is written.

```
terminal screen ──▶ page (structured) ──▶ GUI
                        ▲                  │
                        └──────────────────┘
                          five of seven kinds
```

## What the model is

A softmax regression over **nineteen features** read from the screen itself — **100 parameters**, trained on **434
screens** of one application. It ships in `models/page.json` and runs in Rust; scoring a session needs no Python and no
toolchain.

It is small on purpose. Retraining on 60% more material bought **0.1%** of row accuracy; a twentieth feature was worse on
every screen family; a sixteen-times-larger network added **0.4 points**. The evidence says the assembly generalises, not
the model — so the assembly is where the work went. The numbers, the ceilings and the failures are in
[docs/model-card.md](docs/model-card.md).

## What a page holds

Seven kinds, and nothing invented beyond them:

- **`Block`** — a run of rows that reads as one thing, with its role, lines and per-span styling (bold, italic,
  underline, reverse, colours as `indexed:N` or `rgb:R,G,B`).
- **`ToolCall`** — a marked row (`♦` from one agent, `•` from another) with its tool, target, status, and for an edit the
  `+A -R` counts the application prints.
- **`Thought`** — a reasoning heading, `♦ Thought for 3.0s`, ending at its own word.
- **`Meter`** — a quantity against a limit, `1.3K / 500K │ [Dashboard]`. It refuses to invent a limit from a percentage,
  so `15.4K (1%)` stays a block.
- **`Diff`** — rows carrying both a `+` and a `-`, with a per-line kind. Two shapes exist in the corpus: the `+++`/`---`
  kind and a staged rename.
- **`Frame`** — a floating window drawn over the page rather than beside it: a heavy-rule popup and a centred dialog.
- **`Table`** — numbers under headings: `df`, `ls -l`, `ps`, `top`, `dir`. Two vertical strips holding digits in most
  rows, separated by a blank column — read by **values, not padding**, because `df` pads with single spaces and
  `Format-Table` with many ([ADR 0008](docs/adr/0008-a-table-is-numeric-columns.md)).

**The model labels five of the seven.** `Frame` and `Table` are shaped by the rule layer — a floating window is
recognised by its border, a table by its numeric columns — which is the split the whole design rests on: what a thing
*is*, is learned; how it is *shaped*, is written.

Every component says whether it was read from the screen (`seen`) or carried from an earlier page (`carried`), and its id
comes from its own text rather than its position, so it keeps its id when it scrolls.

## Both directions

**TUI → GUI** is the ordinary case: record any program, describe its screens, draw the page as HTML.

**GUI → TUI** is checked too: the rendered document is parsed back and compared with the terminal rendering of the same
frame. Five of the seven kinds round trip. `Meter` does not, because it is drawn as a bar and its number is not in a line
element — a consumer that needs the number reads it from the page, not from the HTML.

## Download

Binaries for Windows, Linux and macOS are attached to the [latest
release](https://github.com/morewebs/UI-LM/releases/latest). Unpack and run `ui-lm-capture --help`.

Three programs come in each archive: `ui-lm-capture` records and describes, `ui-lm-serve` serves a live view and types
back into the program, `ui-lm-validate` compares a recording against an application's own draw buffer. The trained model
travels with them in `models/page.json`, so a download can score and describe a screen without the repository.

## Quickstart

```bash
cargo build --workspace
cargo test --workspace        # 251 tests across 31 suites
```

Record a program, then describe it:

```bash
ui-lm-capture record --rows 50 --cols 160 --out sessions/example -- --some-program
ui-lm-capture page --session sessions/example --model models/page.json    # what the model reads
ui-lm-capture render --session sessions/example --all --out example.html  # the page, as a GUI
```

That is the loop: a program was observed, the model described its screens, and the description was drawn by something
that never saw the terminal. [docs/demo/fixture.html](docs/demo/fixture.html) is a rendering of the checked-in
recording, so the output can be looked at without building anything.

A frame exists only at a **settled** screen — 150 ms of quiet — so an animating program yields frames at its pauses.

## The live view

Two processes: one records, one serves the recording and forwards what you type.

```bash
ui-lm-capture record --out sessions/live --control keys.txt -- --some-program
ui-lm-serve --session sessions/live --keys keys.txt --port 8787 --model models/page.json
# open http://127.0.0.1:8787/
```

The browser shows screens as they settle, and the box at the bottom types into the program: text goes to `keys.txt`, the
recorder forwards it to the terminal. A file rather than a socket on purpose — no port to open, nothing reachable from
another machine. It listens on this machine only.

## Measuring it

Every claim here came from a command, and the commands are in the repository:

```bash
ui-lm-capture score --session sessions/example --model models/page.json      # model beside the floor
ui-lm-capture disagree --session sessions/example --model models/page.json   # rows the model labels differently
ui-lm-capture provenance --session sessions/example                         # carried or seen, honestly?
ui-lm-capture audit --model models/page.json -- sessions/                   # pages that contradict their screens
ui-lm-capture dataset --session sessions/example --out pairs.jsonl          # screen text and the page, as pairs
ui-lm-capture summarize -- sessions/                                        # what a corpus holds
ui-lm-capture self-check --session sessions/example                         # what the code built, apart from what the model judged
ui-lm-capture fields --left a --right b                                     # the parts `compare` does not look at
ui-lm-capture baseline --session sessions/example                           # the text-only floor, printed
ui-lm-capture tui --session sessions/example --seq 0                        # the page drawn back as terminal text
ui-lm-capture frames --session sessions/example                             # one line per screen, with its kinds
ui-lm-capture compare --left a --right b                                    # two recordings, side by side
ui-lm-capture audit -- sessions/                                            # pages that contradict their screens
powershell -File scripts/check-all.ps1        # every evaluation set, both directions
powershell -File scripts/check-everything.ps1 # all 25 checks, about 22 minutes
```

## What is proven

Against **105 recordings and 1,580 frames** from eleven distinct commands — two agent CLIs, `whiptail`, `vim`, `top`,
`df`, `man`, `less`, `git`, `cmd` and PowerShell — gathered on two machines:

- **The reader and the model both describe every evaluation frame, and both are consistent with their screens.** Six
  evaluation sets, each against a floor — a converter that only splits paragraphs. The floors are printed by
  `scripts/check-all.ps1`, because a floor of 100% means a set cannot fail and proves nothing about the vocabulary.
- **The page can be drawn back.** Terminal text returns every line of content on every recording measured, split screens
  included; the rendered HTML can be turned back into the screen for five of seven kinds.
- **Fifteen hard screens** — paned agents, a floating dialog, a completion popup, a blank split — each with a score, a
  ceiling on components, and where it applies the kind it exists for or the kinds it cannot hold.
- **Provenance is honest, and pinned.** One component in the whole corpus is marked `carried` on its first appearance;
  the number and its cause are in `scripts/check-corpus.ps1`.

## What is not proven, and is not claimed

- **The training material is one application's screens.** Every number rests on 434 screens of one agent CLI, because
  only agents draw the components this vocabulary is for. The model reads other programs correctly; there was nothing in
  them to train on.
- **The evaluation material is this machine's**, mostly chosen by whoever built the model. Fresh batches recorded after
  training are the strongest claim available, and it is a modest one.
- **A live view is not continuous.** A frame exists at a settled screen, so a fast program yields fewer moments than it
  drew.
- **A table's rule has two false positives.** The values rule matches nineteen of the corpus's 105 recordings, and
  two of them are agent screens of three or four rows where prose aligns by accident. The kind requires a block of
  its own to keep them out, and the boundary is measured in `scripts/measure-tables.py`.
- **`Meter` has one observed producer.** Every meter in the corpus comes from an agent printing a budget; a drawn
  progress bar is deliberately not one.

The longer account — every attempt, every rejected hypothesis, every cost — is in
[docs/reports/2026-10-28-model.md](docs/reports/2026-10-28-model.md).

## Where things are

| | |
|---|---|
| `crates/model` | nineteen features, the weights, inference |
| `crates/pages` | the reader: screen → page, the A2UI envelope, the catalogue |
| `crates/frames` | the recording format: run-length cells with character and style |
| `crates/capture` | pty recording and the CLI |
| `crates/render` | page → HTML, page → terminal text |
| `crates/serve` | the live view, and typing back into the program |
| `scripts/` | 25 checks, the trainer, and the measurement scripts |

Design and decisions are in the repository: [the design spec](docs/superpowers/specs/2026-10-27-ui-lm-design.md), [the
glossary](GLOSSARY.md), [the catalogue reference](docs/catalogue/moreweb.terminal-v1.md), and the ADRs — [the cell grid is
the model's input](docs/adr/0001-cell-grid-as-model-input.md), [A2UI's envelope with our
catalogue](docs/adr/0002-a2ui-envelope-custom-catalogue.md), and [a table is numeric
columns](docs/adr/0008-a-table-is-numeric-columns.md).

## Picking this up

[State of the project](docs/reports/2026-10-28-state-of-the-project.md) is the plain-language account. [HANDOFF.md](HANDOFF.md)
is the short version: where everything lives, how to verify it, what is proven, the fragile spots, and the three open
threads with the step that continues each.

## Licence

Apache-2.0. See [LICENSE](LICENSE).
