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

**The capture script is `scripts/capture-w1.ps1`**, and it is in `scripts/` — it is the
instrument every session in
[`docs/reports/2026-10-27-w1-capture.md`](../docs/reports/2026-10-27-w1-capture.md) came
from. It cannot record the patched fork: the application's path is fixed near the top
of the script (`$GrokExe = 'C:\Users\moreweb\.grok\bin\grok.exe'`) and it takes no
parameter for another binary. So the per-scenario commands below are the K5
instrument, **concrete arguments and nothing left to fill in**, and they obey the four
rules the W1 capture paid for:

- **The folder-trust card eats the first keystroke.** A directory the application has
  not been told to trust opens on *"Do you trust the contents of this directory?"*, and
  while that card is up keystrokes aimed at the prompt are swallowed. Every key list
  below therefore begins with an Enter at 4500 ms, which answers the card. (In a
  directory the application already trusts, that Enter is an empty line at the prompt,
  which none of these three screens depend on.)
- **`--cwd` is passed twice.** `portable-pty` 0.9 runs a child whose command names no
  directory in the user's home directory, so the recorder is given `--cwd $Scratch`
  *and* the application is given its own `--cwd $Scratch` after `--`. Without both,
  the session records this repository rather than the scratch project — which is what
  the first pass of the W1 capture did, and one scenario created a file in the user's
  home because of it.
- **`NO_COLOR` is cleared.** This harness exports `NO_COLOR=1`, the application
  honours it by drawing without colour, and a capture taken under it carries one
  default colour and no styling. Under it the mandatory styled-wide-glyph scenario
  would compare clean while proving nothing, so every block below removes the
  variable from the environment it runs in.
- **The dump file is removed before the run.** The fork appends, so a run that follows
  a run would put two runs' draws in one file.

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

`grok` takes no arguments here beyond the directory it is being pointed at: it is the
program after `--`, and the startup scenario is the bare TUI. A scenario that does need
application arguments appends them after the binary path, still after `--`.

```powershell
cargo build -p ui-lm-capture
New-Item -ItemType Directory -Force sessions/k5 | Out-Null
$Scratch = Join-Path $env:TEMP 'ui-lm-k5\cwd\probe'
New-Item -ItemType Directory -Force -Path $Scratch | Out-Null
# The prologue Enter answers the folder-trust card. See the rules above.
[IO.File]::WriteAllText("$PWD\sessions\k5\probe.keys", "4500 \r", (New-Object System.Text.UTF8Encoding($false)))
Remove-Item sessions/k5/probe-buffer.jsonl -ErrorAction SilentlyContinue
Remove-Item Env:\NO_COLOR -ErrorAction SilentlyContinue
$env:GROK_FRAME_DUMP = "$PWD\sessions\k5\probe-buffer.jsonl"

.\target\debug\ui-lm-capture.exe record --rows 50 --cols 160 --out sessions/k5/probe `
  --script sessions/k5/probe.keys --cwd $Scratch `
  -- ..\grok-build-k5\target\release\grok.exe --cwd $Scratch

Remove-Item Env:GROK_FRAME_DUMP
```

The `Remove-Item` before the run is not housekeeping: the fork **appends**, so a dump
left by an earlier run would put two runs' draws in one file. The recorder passes its
own environment to the child, so the fork inherits `GROK_FRAME_DUMP` through the pty;
`Remove-Item Env:` afterwards stops the next session appending to this one's dump.

### Scenario 2 — a styled full-width glyph (mandatory)

The keystroke file is the prologue Enter at 4500 ms that answers the trust card, and
then a 1500 ms pause before 你好 is typed into the prompt. The first segment is
`"4500 \r"` — a backslash and an `r`, which `read_script` turns into a carriage return,
**not** a real CR before the line ending, which it would strip as a CRLF and then send
nothing. `[char]` and `WriteAllText` are used so the file is UTF-8 **without** a BOM,
which `read_script` would otherwise send as a keystroke.

```powershell
New-Item -ItemType Directory -Force sessions/k5 | Out-Null
$Scratch = Join-Path $env:TEMP 'ui-lm-k5\cwd\wide-glyph'
New-Item -ItemType Directory -Force -Path $Scratch | Out-Null
[IO.File]::WriteAllText("$PWD\sessions\k5\wide-glyph.keys", "4500 \r" + "`r`n" + "1500 " + [char]0x4F60 + [char]0x597D, (New-Object System.Text.UTF8Encoding($false)))
Remove-Item sessions/k5/wide-glyph-buffer.jsonl -ErrorAction SilentlyContinue
Remove-Item Env:\NO_COLOR -ErrorAction SilentlyContinue
$env:GROK_FRAME_DUMP = "$PWD\sessions\k5\wide-glyph-buffer.jsonl"

.\target\debug\ui-lm-capture.exe record --rows 50 --cols 160 --out sessions/k5/wide-glyph `
  --script sessions/k5/wide-glyph.keys --cwd $Scratch `
  -- ..\grok-build-k5\target\release\grok.exe --cwd $Scratch

Remove-Item Env:GROK_FRAME_DUMP
```

1500 ms is the pause *before* the keystroke, long enough for the TUI to have drawn its
first screen after the card is answered; raise it if the bytes arrive while the
application is still starting. End the run as described below once the glyph is on
screen.

The scenario has done its job only if the dump holds the glyph **and** its continuation
cell, in one styled run:

```powershell
(Select-String -Path sessions/k5/wide-glyph-buffer.jsonl -Pattern '"symbol":"\u4f60"' | Measure-Object).Count
```

Expected: at least `1` — that many draws contain the glyph. Then look at it: the cell
immediately to its right must be `"symbol":""`, and both cells must carry the same
non-`reset` `fg` (or `bg`). That is the wide-glyph question, and step 5 turns that pair
into a verdict.

If the prompt box draws typed text in `reset` — plausible, since an input box may not
style what is typed into it — the substitute screen is the application's own rendered
answer: run the two glyphs as a prompt (`1500 ` + 你好 + `\r` in the same key file) and
use the finished turn, whose transcript text is drawn in the theme's own colour rather
than `reset`; if that comes back `reset` too, the continuation-column pair is untestable
in this build and K5 cannot be cleared — which is a finding to record, never a reason to
widen an ignore mask.

### Scenario 3 — a plain prompt (the single-width control)

```powershell
New-Item -ItemType Directory -Force sessions/k5 | Out-Null
$Scratch = Join-Path $env:TEMP 'ui-lm-k5\cwd\plain-prompt'
New-Item -ItemType Directory -Force -Path $Scratch | Out-Null
[IO.File]::WriteAllText("$PWD\sessions\k5\plain-prompt.keys", "4500 \r" + "`r`n" + "1500 hello, grok", (New-Object System.Text.UTF8Encoding($false)))
Remove-Item sessions/k5/plain-prompt-buffer.jsonl -ErrorAction SilentlyContinue
Remove-Item Env:\NO_COLOR -ErrorAction SilentlyContinue
$env:GROK_FRAME_DUMP = "$PWD\sessions\k5\plain-prompt-buffer.jsonl"

.\target\debug\ui-lm-capture.exe record --rows 50 --cols 160 --out sessions/k5/plain-prompt `
  --script sessions/k5/plain-prompt.keys --cwd $Scratch `
  -- ..\grok-build-k5\target\release\grok.exe --cwd $Scratch

Remove-Item Env:GROK_FRAME_DUMP
```

### Ending a run, and what to check

The cleanest ending is to let the application leave: the W1 capture found exactly one
way out — `/exit` typed at the prompt — after Ctrl-C, Ctrl-D and a typed `/quit` were
all ignored. Add it as the last line of the key file and the child exits, so the
recorder ends on `(the program exited)` with no interrupt involved and no live child to
close by hand:

```powershell
# Append to the scenario's key file before running it, 1200 ms after the screen you want:
Add-Content -LiteralPath sessions/k5/wide-glyph.keys -Value "`r`n1200 /exit\r"
```

Both sides stop at the same moment either way, so the last frame and the last draw of
the dump are still the same screen — and if the application will not leave, **Ctrl-C in
the console window that started the run** is the fallback: the recorder claims the
interrupt, finishes the session and writes `session.json`, and prints

```
recorded 34 frame(s) to sessions/k5/probe (interrupted; frames settled up to the last 150ms of quiet)
```

Exit code 0 means the session was written — not that the screen was the one you wanted.

```powershell
# Name the scenario you actually ran: probe (startup), wide-glyph, or plain-prompt.
# A wide-glyph run must be checked against wide-glyph-buffer.jsonl, not the probe's.
$Session = 'wide-glyph'
Get-ChildItem "sessions/k5/$Session"
Get-Content "sessions/k5/$Session-buffer.jsonl" -Tail 1 |
  ForEach-Object { $_.Substring(0, $_.IndexOf('"cells"')) }
```

Expected: `frames.jsonl` and `session.json` in `sessions/k5/<the scenario you ran>`, and
a last line of `sessions/k5/<that scenario>-buffer.jsonl` whose head reads
`{"seq":34,"rows":50,"cols":160,` — the same 50x160 the session declares, with whatever
draw number the run ended on. A dump that declares another shape means the fork's
buffer is not the recorder's screen, and step 5 will refuse the pair.

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

They are also **not** a way past a damaged dump. The validator reads every line of the
dump before it looks at `--theirs-seq` or `--ours-seq`, and an unreadable line — the
usual one being a last line cut short when the run was killed — fails that read with
the file name and the line number. So a dump whose final draw is torn cannot be
salvaged by naming the draw before it: nothing in that file is compared at all. The
fix is to remove the dump and take the session again (the fork appends, so a re-run
would otherwise land beside the damage), never a `--theirs-seq` that steps back from
it.

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
