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
# From the workspace root; the clone must NOT be inside the ui-lm repository.
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

```powershell
# From this repository's root. The recorder passes its own environment to the child,
# so the fork inherits GROK_FRAME_DUMP through the pty.
New-Item -ItemType Directory -Force sessions/w1 | Out-Null
Remove-Item sessions/w1/startup-buffer.jsonl -ErrorAction SilentlyContinue
$env:GROK_FRAME_DUMP = "$PWD\sessions\w1\startup-buffer.jsonl"

cargo run -p ui-lm-capture -- record --rows 50 --cols 160 `
  --out sessions/w1/startup --script <the same --script the W1 capture used> `
  -- ..\grok-build-k5\target\release\grok <the same arguments as the W1 capture>

Remove-Item Env:GROK_FRAME_DUMP
```

The scenario must be the same one the W1 capture used: same `--rows`/`--cols`, same
`--script`, same environment (the recorder sets `TERM=xterm-256color` and
`COLORTERM=truecolor` itself), same application arguments. A session whose final
screen cannot match is not evidence about the emulator, it is evidence about the
scenario.

## 5. Compare

```powershell
cargo run -p ui-lm-validate -- compare `
  --ours sessions/w1/startup `
  --theirs sessions/w1/startup-buffer.jsonl
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
ours:   sessions/w1/startup — frame 34 of 34 (seq 33), 50x160
theirs: sessions/w1/startup-buffer.jsonl — draw 71 of 71 (seq 70), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
```

On a disagreement — exit code non-zero:

```
session: grok started 2026-10-27T10:00:00Z, recorded at 50x160
ours:   sessions/w1/startup — frame 34 of 34 (seq 33), 50x160
theirs: sessions/w1/startup-buffer.jsonl — draw 71 of 71 (seq 70), 50x160
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
from a different scenario. A `mismatched: 0` on one session is a data point; three
are the criterion. Append to the session note, for each session:

- the session id, the fork's commit from step 1, and the patched binary's build;
- the scenario, `--rows`/`--cols`, and the script used;
- the exact `compare` command and its complete output;
- any residual, explained cell by cell;
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
