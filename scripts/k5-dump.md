# K5: comparing our frames against the application's own draw buffer

Kill criterion K5 asks one question: **does the VT100 emulator see the screen the
application drew?** The rig answers it by recording a session from a patched build of
the application that also writes out the `Buffer` it drew, and then comparing the
two with `ui-lm-validate compare`. If the emulator's screen and the application's
buffer disagree, every frame, eval number and training pair we produce is a
description of a screen that never existed.

The patch is scaffolding. It exists for the length of one comparison, in its own
clone outside this repository, and is **deleted afterwards** — the rig must never
build against or depend on a forked application.

## What the dump looks like

One JSON object per draw, one per line, appended to `$GROK_FRAME_DUMP`:

```json
{"seq":0,"rows":50,"cols":160,"cells":[{"row":0,"col":0,"symbol":"a","fg":"reset","bg":"reset","bold":false,"italic":false,"underline":false,"reverse":false}]}
```

`seq` is a counter the fork keeps, starting at 0, incremented once per completed
draw. `rows`/`cols` are the buffer area's dimensions. `cells` is the buffer's cells
in row-major order, each naming its own `row`/`col`.

The two sides deliberately share no format: the fork cannot depend on our crate and
our crate must not gain a JSON dependency for a throwaway dump. `ui-lm-validate`
converts each cell into a `ui-lm-frames` `Style` like this:

| ratatui | dump string | `ui_lm_frames::Color` |
| --- | --- | --- |
| `Color::Reset` | `"reset"` | `Color::Default` |
| `Color::Indexed(n)` | `"indexed:n"` | `Color::Indexed(n)` |
| `Color::Rgb(r, g, b)` | `"rgb:r,g,b"` | `Color::Rgb(r, g, b)` |
| a named colour (`Color::Red`, …) | `"indexed:1"` — palette entry 0–15 | `Color::Indexed(1)` |
| `Modifier::BOLD` | `"bold":true` | `Style::bold` |
| `Modifier::ITALIC` | `"italic":true` | `Style::italic` |
| `Modifier::UNDERLINED` | `"underline":true` | `Style::underline` |
| `Modifier::REVERSED` | `"reverse":true` | `Style::reverse` |

A symbol the fork reports as `""` (the column after a full-width glyph) reads as a
space, which is what the emulator's grid holds there. Anything else — an unknown
colour form, a missing field, a line that is not that object — is an error naming
the file and line, never a silent default: a cell neither side described must not
come out as a match.

## 1. Clone the application, outside this repo

```powershell
# From this repository's root, so `..\grok-build-k5` is where steps 4 and 7 look for it.
# The clone must NOT be inside the ui-lm repository.
git clone https://github.com/xai-org/grok-build ../grok-build-k5
cd ../grok-build-k5
git rev-parse HEAD        # record this: the dump and the capture must come from one build
git remote -v
```

Cut a local branch for the patch. Nothing is pushed.

```powershell
git switch -c k5-frame-dump
```

## 2. Add the `frame-dump` feature

`Cargo.toml` of the TUI crate (whichever crate owns `ratatui`):

```toml
[features]
frame-dump = []
```

New file `src/frame_dump.rs`, next to that crate's TUI code:

```rust
//! K5 scaffolding: writes the buffer this application drew as per-cell JSON.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

/// Draws completed so far, which is the `seq` a dump line carries.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Appends one completed draw to the file named by `$GROK_FRAME_DUMP`.
///
/// One line per draw, appended and flushed: a Ctrl-C keeps every draw written so
/// far, and an unfinished last line is refused by the validator rather than
/// silently skipped.
pub fn dump(buffer: &Buffer) {
    let Some(path) = std::env::var_os("GROK_FRAME_DUMP") else {
        return;
    };
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let mut line = String::with_capacity(256 + buffer.content.len() * 96);
    line.push_str(&format!(
        "{{\"seq\":{seq},\"rows\":{},\"cols\":{},\"cells\":[",
        buffer.area.height, buffer.area.width
    ));
    let width = u32::from(buffer.area.width).max(1);
    for (index, cell) in buffer.content.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        line.push_str(&format!(
            "{{\"row\":{},\"col\":{},\"symbol\":\"{}\",\"fg\":\"{}\",\"bg\":\"{}\",\
             \"bold\":{},\"italic\":{},\"underline\":{},\"reverse\":{}}}",
            index as u32 / width,
            index as u32 % width,
            escape(cell.symbol()),
            colour(cell.fg),
            colour(cell.bg),
            cell.modifier.contains(Modifier::BOLD),
            cell.modifier.contains(Modifier::ITALIC),
            cell.modifier.contains(Modifier::UNDERLINED),
            cell.modifier.contains(Modifier::REVERSED),
        ));
    }
    line.push_str("]}\n");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// A colour in the dump's vocabulary.
fn colour(colour: Color) -> String {
    match colour {
        Color::Reset => "reset".to_owned(),
        Color::Indexed(index) => format!("indexed:{index}"),
        Color::Rgb(r, g, b) => format!("rgb:{r},{g},{b}"),
        // ratatui's named colours are palette entries 0–15 in ANSI order.
        named => format!("indexed:{}", palette_index(named)),
    }
}

/// The palette index of a named colour.
fn palette_index(colour: Color) -> u8 {
    match colour {
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        _ => 0,
    }
}

/// A symbol as JSON string contents.
///
/// Only the characters JSON requires are escaped; everything else, including
/// non-ASCII, is written as the UTF-8 it already is. `char::escape_default` is
/// *not* usable here: it writes `\u{4f60}`, which is not JSON.
fn escape(symbol: &str) -> String {
    let mut out = String::with_capacity(symbol.len());
    for ch in symbol.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}
```

Declare it (`lib.rs`/`main.rs` of that crate): `#[cfg(feature = "frame-dump")] pub mod frame_dump;`

Then call it once per completed draw, from the one place the TUI draws — the closure
passed to `Terminal::draw`, whose `Frame` is the buffer about to be flushed:

```rust
terminal.draw(|frame| {
    // … existing rendering …
    #[cfg(feature = "frame-dump")]
    crate::frame_dump::dump(frame.buffer_mut());
})?;
```

The invariant is what matters if the surrounding code differs: dump the buffer the
draw closure filled, once per completed draw, appended and flushed per line. Do not
dump from a redraw of a partial buffer, and do not dump before the application's own
layout has run.

## 3. Build it

```powershell
cd ../grok-build-k5
cargo build --release --features frame-dump
```

Record the binary's path and version in the session note beside the commit from
step 1: the dump is only evidence about *this* build of the application.

## 4. Run one session, capturing both sides at once

Both artefacts come out of **one run**: the recorder records the patched binary, and
the patched binary dumps its buffer, so the two describe the same execution. Two
separate runs would compare two screens that were never on screen together.

**Fixed geometry.** Every K5 session is recorded at `--rows 50 --cols 160` — the W1
geometry, a 50x160 buffer of 8000 cells — and that pair is written into every command
below rather than left to be remembered. The comparison is cell by cell and refuses
two screens of different shapes, so a session recorded at any other size is not
evidence at all.

**The capture script is `scripts/capture-w1.ps1`** — the Task 10 script that drives the
W1 scenarios through this same recorder. It is not in `scripts/` yet (as of
`e34b2ca`), so the fallback here is a manual invocation **per scenario, with concrete
arguments and nothing left to fill in**. Once `scripts/capture-w1.ps1` lands, take its
per-scenario keystroke files, keep the geometry, keep the dump-before-run rule, and
prefer its invocations to these.

The blocks below are Windows PowerShell and run as written in `powershell.exe` (5.1,
this machine's host) and in `pwsh` 7: nothing here needs `-NoNewline` (which 5.1
lacks), every file is UTF-8 **without** a BOM, and the glyphs are written with
`[char]` so the pasted text stays ASCII. Every command is run **from this repository's
root**, which is what `$PWD`, `sessions/` and `target/` in them refer to.

### The scenarios the K5 run must cover

| # | scenario | the screen it must produce | status |
| --- | --- | --- | --- |
| 1 | startup | the opening screen, nothing typed | one of the three |
| 2 | **styled full-width glyph** | a CJK glyph inside a coloured span or box, with its continuation cell inside the same run | **mandatory** |
| 3 | plain prompt | a typed ASCII prompt in the input box | the single-width control for #2 |

Scenario 2 is not optional and not interchangeable. Whether ratatui gives a wide
glyph's continuation cell the glyph's own style, or resets it, is **unverified**; our
adapter deliberately inherits the head cell's style onto the continuation cell, and that
pair of cells is the one place it guesses. If K5 mismatches on a continuation column,
Task 3's inheritance ruling is what is wrong, and the fix is decided then — never
absorbed by widening an ignore mask.

### Scenario 1 — startup (the opening screen)

`grok` takes no arguments here: it is the program after `--`, and the startup scenario
is the bare TUI. A scenario that does need application arguments appends them after the
binary path, still after `--`.

```powershell
cargo build -p ui-lm-capture
New-Item -ItemType Directory -Force sessions/k5 | Out-Null
Remove-Item sessions/k5/probe-buffer.jsonl -ErrorAction SilentlyContinue
$env:GROK_FRAME_DUMP = "$PWD\sessions\k5\probe-buffer.jsonl"

.\target\debug\ui-lm-capture.exe record --rows 50 --cols 160 --out sessions/k5/probe `
  -- ..\grok-build-k5\target\release\grok.exe

Remove-Item Env:GROK_FRAME_DUMP
```

The `Remove-Item` before the run is not housekeeping: the fork **appends**, so a dump
left by an earlier run would put two runs' draws in one file. The recorder passes its
own environment to the child, so the fork inherits `GROK_FRAME_DUMP` through the pty;
`Remove-Item Env:` afterwards stops the next session appending to this one's dump.

### Scenario 2 — a styled full-width glyph (mandatory)

The keystroke file is a 1500 ms pause and then 你好 typed into the prompt. `[char]` and
`WriteAllText` are used so the file is exactly `1500 ` followed by the UTF-8 bytes of
the two glyphs — no BOM, which `read_script` would otherwise send as a keystroke.

```powershell
New-Item -ItemType Directory -Force sessions/k5 | Out-Null
[IO.File]::WriteAllText("$PWD\sessions\k5\wide-glyph.keys", "1500 " + [char]0x4F60 + [char]0x597D, (New-Object System.Text.UTF8Encoding($false)))
Remove-Item sessions/k5/wide-glyph-buffer.jsonl -ErrorAction SilentlyContinue
$env:GROK_FRAME_DUMP = "$PWD\sessions\k5\wide-glyph-buffer.jsonl"

.\target\debug\ui-lm-capture.exe record --rows 50 --cols 160 --out sessions/k5/wide-glyph `
  --script sessions/k5/wide-glyph.keys `
  -- ..\grok-build-k5\target\release\grok.exe

Remove-Item Env:GROK_FRAME_DUMP
```

1500 ms is the pause *before* the keystroke, long enough for the TUI to have drawn its
first screen; raise it if the bytes arrive while the application is still starting.
End the run as described below once the glyph is on screen.

The scenario has done its job only if the dump holds the glyph **and** its continuation
cell, in one styled run:

```powershell
(Select-String -Path sessions/k5/wide-glyph-buffer.jsonl -Pattern '"symbol":"\u4f60"' | Measure-Object).Count
```

Expected: at least `1` — that many draws contain the glyph. Then look at it: the cell
immediately to its right must be `"symbol":""`, and both cells must carry the same
non-`reset` `fg` (or `bg`). That is the wide-glyph question, and step 5 turns that pair
into a verdict.

### Scenario 3 — a plain prompt (the single-width control)

```powershell
New-Item -ItemType Directory -Force sessions/k5 | Out-Null
[IO.File]::WriteAllText("$PWD\sessions\k5\plain-prompt.keys", "1500 hello, grok", (New-Object System.Text.UTF8Encoding($false)))
Remove-Item sessions/k5/plain-prompt-buffer.jsonl -ErrorAction SilentlyContinue
$env:GROK_FRAME_DUMP = "$PWD\sessions\k5\plain-prompt-buffer.jsonl"

.\target\debug\ui-lm-capture.exe record --rows 50 --cols 160 --out sessions/k5/plain-prompt `
  --script sessions/k5/plain-prompt.keys `
  -- ..\grok-build-k5\target\release\grok.exe

Remove-Item Env:GROK_FRAME_DUMP
```

### Ending a run, and what to check

The application does not exit by itself, so end the run with **Ctrl-C in the console
window that started it**: the recorder claims the interrupt, finishes the session and
writes `session.json`, and prints

```
recorded 34 frame(s) to sessions/k5/probe (interrupted; frames settled up to the last 150ms of quiet)
```

Exit code 0 means the session was written — not that the screen was the one you wanted.
If a scenario can ask the application to exit instead (a quit keystroke as a final
`--script` line), prefer that: the recorder then ends on `(the program exited)` with no
interrupt involved, and there is no live child to close by hand.

```powershell
Get-ChildItem sessions/k5/probe
Get-Content sessions/k5/probe-buffer.jsonl -Tail 1 | ForEach-Object { $_.Substring(0, $_.IndexOf('"cells"')) }
```

Expected: `frames.jsonl` and `session.json` in the directory, and a last dump line whose
head reads `{"seq":34,"rows":50,"cols":160,` — the same 50x160 the session declares,
with whatever draw number the run ended on. A dump that declares another shape means
the fork's buffer is not the recorder's screen, and step 5 will refuse the pair.

## 5. Compare

One command per scenario, each naming the session it is evidence about:

```powershell
cargo build -p ui-lm-validate

.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/probe --theirs sessions/k5/probe-buffer.jsonl
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/wide-glyph --theirs sessions/k5/wide-glyph-buffer.jsonl
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/plain-prompt --theirs sessions/k5/plain-prompt-buffer.jsonl
```

By default this compares **our last frame against the fork's last draw**. That is the
one screen the two sides are guaranteed to share: our recorder frames the final
settled screen when the child's output ends, and the fork's last draw is that same
screen. It is **one strong data point per session, not a frame-by-frame sequence
alignment** — a session of forty draws may be thirty frames, and frame 12 has no
principled relationship to draw 12.

`--ours-seq N` and `--theirs-seq M` name one screen by its own `seq` on each side
when a specific middle screen is wanted. They are not an index into an aligned
sequence, and a `seq` the run does not have is an error rather than a fallback.

Output on a clean comparison — exit code 0:

```
session: grok started 2026-10-27T10:00:00Z, recorded at 50x160
ours:   sessions/k5/probe — frame 34 of 34 (seq 33), 50x160
theirs: sessions/k5/probe-buffer.jsonl — draw 71 of 71 (seq 70), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
```

On a disagreement — exit code non-zero:

```
session: grok started 2026-10-27T10:00:00Z, recorded at 50x160
ours:   sessions/k5/probe — frame 34 of 34 (seq 33), 50x160
theirs: sessions/k5/probe-buffer.jsonl — draw 71 of 71 (seq 70), 50x160
total cells: 8000
mismatched:  3
first mismatch: 12,34: ours='a' theirs='b'
K5: the emulator's screen and the draw buffer disagree on 3 of 8000 cell(s)
```

Nothing is masked: `Frame::volatile` is our own recorder's claim about our own
frames, and letting it silence cells would let the mask hide exactly the
disagreement K5 exists to find. If a mismatch is genuinely a glyph this rig knows
renders differently, it is an explained residual in the note — per cell, with a
reason — not something the comparison silently dropped.

## 6. Verdict and record

**K5 is cleared only when this passes on at least three sessions**, each captured
from a different scenario in the table in step 4, and **one of them must be the styled
full-width glyph** — a run without that scenario leaves the adapter's one unverified
guess untested, and cannot clear K5 however many other screens match. A `mismatched: 0`
on one session is a data point; three are the criterion. Append to the session note,
for each session:

- the session id, the fork's commit from step 1, and the patched binary's build;
- the scenario from the table, `--rows 50 --cols 160`, and the `--script` file used;
- the exact `compare` command and its complete output;
- any residual, explained cell by cell — and for the wide-glyph session, whether the
  mismatch count is zero, so the continuation-column pair is settled either way;
- the SHA of this repository when the comparison was run.

A residual that cannot be explained per cell is a K5 failure, not a residual.

## 7. Delete the fork

```powershell
Remove-Item -Recurse -Force ../grok-build-k5
git status --short              # only this file and the validator are changed
git diff --stat -- Cargo.toml Cargo.lock   # must be empty: no dependency, no lock churn
```

Then confirm the patch left nothing behind in this repository:

```powershell
Get-ChildItem -Recurse -File crates |
  Select-String -Pattern 'grok-build|frame-dump'
```

Expected: nothing. The only surviving mention of the fork is this recipe, which
exists so the comparison can be repeated — and then deleted again.
