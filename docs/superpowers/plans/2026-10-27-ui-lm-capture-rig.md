# Capture Rig Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record deterministic, validated cell-grid sessions from a running terminal application, so that every later artefact (catalogue, eval, model, runtime) has trustworthy input.

**Architecture:** A Rust Cargo workspace. A PTY driver spawns the application and feeds its bytes into a VT100 emulator; the emulator's screen is snapshotted as a `Frame` (run-length encoded styled cells) whenever output has been quiet for 150 ms. Frames are appended to a JSONL session file alongside the application's own event stream, with volatile spans masked and changed rows diffed. A separate validator compares our frames against the application's own draw buffer (obtained from a throwaway patch) — that comparison is kill criterion K5, and without it no later number means anything.

**Tech Stack:** Rust 2021, `portable-pty` 0.9 (PTY/ConPTY), `vt100` (VT parser + screen model), `serde`/`serde_json`, `clap` 4. No async runtime in the rig: a small polling loop is enough and keeps ordering obvious.

**Spec:** `docs/superpowers/specs/2026-10-27-ui-lm-design.md` (revision 2). This plan implements §7.1 plus the W1 row of §8, and clears kill criterion K5.

---

## Scope

The spec describes four subsystems (rig, catalogue, runtime, model). This plan covers **only the rig**, because §6 D9 makes the catalogue depend on real captured frames: a schema for screens you have not captured is a guess. Three plans follow — catalogue + eval floor, runtime, teacher/model — and each produces working software on its own.

## Global Constraints

Copied from the spec; every task inherits them.

- **Refresh** = a frame after **150 ms** of quiescence. Not every PTY byte (§6 D17).
- **Changed-rows-only input is mandatory.** Every frame after the first must carry a machine-readable diff of which rows changed (§6 D17).
- **Volatile spans are masked and recorded, never silently dropped** (§7.1).
- **Cost is logged per session** — the Grok credit is a bounded budget, split across W1 capture, W3 capture and the G1 demo (§4 C15, §10.10).
- **Windows/ConPTY is the first platform.** The driver must not hard-code Unix assumptions (§10.6).
- **Grok Build is never patched permanently.** The draw-buffer dump exists only for K5 validation and is deleted afterwards (§6 D7, §11).
- **No secrets in the repo.** Nothing in this plan needs a key; if one is ever added it lives in `.est/secrets.toml` (§11).
- **Stage 1 is research.** Record latency (M6) if it is free; never optimise for it here (§4 C17).

## Review Focus

The failure modes a person using this rig will actually meet, most likely first. Each is pinned to the task that owns the code.

1. **Full-width characters (CJK, emoji)** occupy two columns. One `char` per cell column silently corrupts every later geometry claim. → Task 3.
2. **Trailing whitespace and tabs carry meaning.** Box-drawing alignment and right-aligned numbers break when a serialiser trims. → Task 2.
3. **Alt-screen enter/exit and mid-scroll frames.** A frame captured during a scroll is neither the old screen nor the new one, and poisons idempotence training. → Task 5.
4. **Typeahead bursts** produce many frames decoupled from the 150 ms rule; a burst must settle once, not once per byte. → Task 5.
5. **Ambiguous-width glyphs** differ between locales; the same byte stream must not yield two geometries on two machines. → Task 3.

---

## File Structure

```
Projects/UI-LM/
├── Cargo.toml                       workspace manifest
├── crates/
│   ├── frames/                      Frame model, RLE codec, session file I/O
│   │   ├── src/lib.rs
│   │   ├── src/grid.rs              StyledCell, Grid, from/to vt100 Screen
│   │   ├── src/rle.rs               encode/decode
│   │   └── src/session.rs           SessionRecord, JSONL writer/reader, cost log
│   ├── capture/                     PTY driver, quiescence, masking, CLI
│   │   ├── src/main.rs              clap CLI: `record`
│   │   ├── src/driver.rs            spawn, read, resize, lifecycle
│   │   ├── src/quiesce.rs           settle detection, frame emission, changed rows
│   │   └── src/mask.rs              volatile-span classification
│   └── validate/                    K5 comparison against the app's draw buffer
│       ├── src/main.rs              `compare` CLI
│       └── src/compare.rs           frame-vs-buffer diffing
└── tests/fixtures/                  recorded byte streams, expected frames
```

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml`, `crates/frames/Cargo.toml`, `crates/frames/src/lib.rs`, `crates/capture/Cargo.toml`, `crates/capture/src/main.rs`, `crates/validate/Cargo.toml`, `crates/validate/src/main.rs`, `.gitignore`

**Interfaces:**
- Produces: a workspace where `cargo test --workspace` runs, and three binary/library crates named `ui-lm-frames`, `ui-lm-capture`, `ui-lm-validate`.

- [ ] **Step 1: Create the workspace manifest**

`Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/frames", "crates/capture", "crates/validate"]

[workspace.package]
edition = "2021"
rust-version = "1.85"
license = "Apache-2.0"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Create the three crates**

`crates/frames/Cargo.toml` uses `edition.workspace = true`, `serde.workspace = true`, `serde_json.workspace = true`. `crates/capture/Cargo.toml` adds `ui-lm-frames = { path = "../frames" }`, `portable-pty = "0.9"`, `vt100 = "0.15"`, `clap = { version = "4", features = ["derive"] }`. `crates/validate/Cargo.toml` adds `ui-lm-frames = { path = "../frames" }` and `clap`.

- [ ] **Step 3: Write a placeholder test that proves the workspace builds**

In `crates/frames/src/lib.rs`:

```rust
pub fn crate_ready() -> bool { true }

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_builds() { assert!(super::crate_ready()); }
}
```

- [ ] **Step 4: Run it**

Run: `cargo test --workspace`
Expected: PASS, three crates compiled.

- [ ] **Step 5: Add `.gitignore` and commit**

`.gitignore` must contain `target/`, `*.jsonl`, `sessions/`, `frame-store/`.

```bash
git add Cargo.toml crates .gitignore
git commit -m "chore: scaffold ui-lm cargo workspace"
```

---

### Task 2: Frame model and run-length codec

**Files:**
- Create: `crates/frames/src/grid.rs`, `crates/frames/src/rle.rs`
- Modify: `crates/frames/src/lib.rs`
- Test: inline `#[cfg(test)]` modules in both files

**Interfaces:**
- Produces:
  - `pub struct Style { fg: Color, bg: Color, bold: bool, italic: bool, underline: bool, reverse: bool }` where `Color` is `Default | Indexed(u8) | Rgb(u8,u8,u8)`.
  - `pub struct StyledCell { ch: char, style: Style }`
  - `pub type Grid = Vec<Vec<StyledCell>>` — `grid[row][col]`, always exactly `rows × cols`, padded with `StyledCell { ch: ' ', .. }`.
  - `pub fn encode_grid(grid: &Grid) -> Vec<CellRun>`
  - `pub fn decode_grid(runs: &[CellRun], rows: u16, cols: u16) -> Grid`
  - `pub struct CellRun { row: u16, col: u16, len: u16, ch: char, style: Style }`

- [ ] **Step 1: Write the failing tests**

`crates/frames/src/rle.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Style, StyledCell};

    #[test]
    fn roundtrip_preserves_every_cell_including_trailing_spaces() {
        // A 2x4 grid whose last row is entirely spaces but styled (reverse video),
        // which is what a selected-but-empty list row looks like.
        let mut g = vec![vec![StyledCell::blank(); 4]; 2];
        g[0][0] = StyledCell::new('a');
        g[0][1] = StyledCell::new('│');           // box drawing
        g[1][3] = StyledCell::new(' ');           // trailing space that matters
        g[1][3].style = Style { reverse: true, ..Style::default() };

        let runs = encode_grid(&g);
        let back = decode_grid(&runs, 2, 4);
        assert_eq!(back, g);
        assert_eq!(back[1].len(), 4, "trailing cells must survive encoding");
        assert!(back[1][3].style.reverse, "style must survive on a space");
    }

    #[test]
    fn identical_neighbours_collapse_into_one_run() {
        let g = vec![vec![StyledCell::new('x'); 8]];
        let runs = encode_grid(&g);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].len, 8);
    }

    #[test]
    fn decode_pads_short_rows_with_blanks() {
        let back = decode_grid(&[], 2, 3);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].len(), 3);
        assert_eq!(back[0][0].ch, ' ');
    }
}
```

`crates/frames/src/grid.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_is_a_space_with_default_style() {
        let c = StyledCell::blank();
        assert_eq!(c.ch, ' ');
        assert_eq!(c.style, Style::default());
    }

    #[test]
    fn cells_compare_by_ch_and_style() {
        let mut a = StyledCell::new('a');
        let b = StyledCell::new('a');
        assert_eq!(a, b);
        a.style.bold = true;
        assert_ne!(a, b);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ui-lm-frames`
Expected: FAIL — `StyledCell`, `Style`, `encode_grid` not found.

- [ ] **Step 3: Implement `grid.rs` and `rle.rs`**

`StyledCell` and `Style` derive `Clone, Debug, PartialEq, Eq, Serialize, Deserialize`. `Color` likewise, with `#[serde(tag = "kind", rename_all = "snake_case")]`. `encode_grid` walks each row left to right and starts a new run whenever `ch` or `style` differs from the previous cell. `decode_grid` fills a `rows × cols` grid of blanks, then stamps each run; runs extending past `cols` are truncated (a wide glyph at the right edge).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p ui-lm-frames`
Expected: PASS, five tests.

- [ ] **Step 5: Commit**

```bash
git add crates/frames
git commit -m "feat(frames): styled cell grid with run-length codec"
```

---

### Task 3: VT100 screen → Grid adapter

**Files:**
- Modify: `crates/frames/src/grid.rs`
- Test: inline module

**Interfaces:**
- Consumes: `StyledCell`, `Style`, `Color`, `Grid` from Task 2.
- Produces: `pub fn grid_from_screen(screen: &vt100::Screen, cols: u16) -> Grid`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod vt_tests {
    use super::*;
    use vt100::Parser;

    fn feed(bytes: &[u8], rows: u16, cols: u16) -> Grid {
        let mut p = Parser::new(rows, cols, 0);
        p.process(bytes);
        grid_from_screen(p.screen(), cols)
    }

    #[test]
    fn reads_characters_and_colours() {
        let g = feed(b"\x1b[31mR\x1b[0m.", 1, 4);
        assert_eq!(g[0][0].ch, 'R');
        assert_eq!(g[0][0].style.fg, Color::Indexed(1));
        assert_eq!(g[0][1].style.fg, Color::Default);
    }

    #[test]
    fn a_full_width_ideograph_occupies_two_columns_and_the_tail_is_a_continuation() {
        // A CJK glyph plus an ASCII letter on a 6-wide row: the letter must sit at
        // column 2, not column 1. A one-char-per-column reader fails this test.
        let g = feed("\u{4f60}\u{597d}x".as_bytes(), 1, 6);
        assert_eq!(g[0][0].ch, '\u{4f60}');
        assert_eq!(g[0][3].ch, 'x');
        assert_eq!(g[0].len(), 6);
    }

    #[test]
    fn rows_are_always_padded_to_the_declared_width() {
        let g = feed(b"hi", 3, 5);
        assert_eq!(g.len(), 3);
        assert!(g.iter().all(|r| r.len() == 5));
        assert_eq!(g[2][0].ch, ' ');
    }

    #[test]
    fn tabs_expand_to_the_next_stop_instead_of_being_dropped() {
        let g = feed(b"a\tb", 1, 10);
        assert_eq!(g[0][0].ch, 'a');
        assert_eq!(g[0][8].ch, 'b');
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ui-lm-frames grid::vt_tests`
Expected: FAIL — `grid_from_screen` not found.

- [ ] **Step 3: Implement `grid_from_screen`**

Walk `0..rows` and `0..cols`; call `screen.cell(row, col)`; map `contents()` to the first `char` of the string, and the style accessors (`fgcolor`, `bgcolor`, `bold`, `italic`, `underline`, `inverse`) onto `Style`. A cell whose contents are empty becomes `' '`. A cell that is the tail of a wide glyph is recorded as `' '` **with a `wide_tail: bool` field set true** — add that field to `StyledCell` in this task, defaulting false, and assert it in the CJK test. Continuation cells must never be silently turned into ordinary blanks. `vt100` already places wide glyphs in the correct column; this adapter must not re-derive widths from `char` width, because locale-dependent ambiguous widths would then differ per machine.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p ui-lm-frames`
Expected: PASS, nine tests.

- [ ] **Step 5: Commit**

```bash
git add crates/frames
git commit -m "feat(frames): adapt vt100 screen to styled grid, wide glyphs preserved"
```

---

### Task 4: PTY driver

**Files:**
- Create: `crates/capture/src/driver.rs`
- Modify: `crates/capture/src/main.rs`
- Test: `crates/capture/tests/driver.rs`

**Interfaces:**
- Consumes: `grid_from_screen` (Task 3), `Grid`.
- Produces:
  - `pub struct SessionConfig { pub command: String, pub args: Vec<String>, pub rows: u16, pub cols: u16, pub env: Vec<(String, String)> }`
  - `pub struct Driver { /* owns pty, child, parser */ }`
  - `impl Driver { pub fn spawn(cfg: &SessionConfig) -> anyhow::Result<Self>; pub fn read_some(&mut self, buf: &mut [u8]) -> std::io::Result<usize>; pub fn screen(&self) -> &vt100::Screen; pub fn write_input(&mut self, bytes: &[u8]) -> std::io::Result<()>; pub fn resize(&mut self, rows: u16, cols: u16) -> anyhow::Result<()>; pub fn try_wait(&mut self) -> anyhow::Result<Option<i32>> }`

- [ ] **Step 1: Write the failing test**

`crates/capture/tests/driver.rs` — spawn a program that prints a known ANSI string and exits, read until EOF, assert the screen content. On Windows use `cmd /c`, elsewhere `sh -c`; keep the assertion platform-neutral by printing only ASCII.

```rust
#[test]
fn captures_output_from_a_spawned_process() {
    let cfg = SessionConfig {
        command: if cfg!(windows) { "cmd".into() } else { "sh".into() },
        args: if cfg!(windows) {
            vec!["/c".into(), "echo hello".into()]
        } else {
            vec!["-c".into(), "printf hello".into()]
        },
        rows: 5, cols: 20,
        env: vec![("TERM".into(), "xterm-256color".into())],
    };
    let mut d = Driver::spawn(&cfg).unwrap();
    let mut buf = [0u8; 1024];
    loop {
        match d.read_some(&mut buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
    let g = grid_from_screen(d.screen(), 20);
    assert!(g[0][0..5].iter().map(|c| c.ch).collect::<String>().contains("hello"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ui-lm-capture --test driver`
Expected: FAIL — `Driver` not found.

- [ ] **Step 3: Implement `driver.rs`**

Use `portable_pty::native_pty_system()` to open a PTY at the configured size, `CommandBuilder` for the command with the env overrides and `TERM=xterm-256color` + `COLORTERM=truecolor` defaults, `slave.spawn_command(cmd)`, then `master.try_clone_reader()`. Set the reader to non-blocking so the loop can decide when to emit frames. Keep one `vt100::Parser` sized `rows × cols` with scrollback 0 (the rig records the viewport; scrollback is the app's own concern). `read_some` processes bytes into the parser and returns the count.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p ui-lm-capture --test driver`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/capture
git commit -m "feat(capture): pty driver over portable-pty plus vt100 parser"
```

---

### Task 5: Quiescence, frame emission, changed rows

**Files:**
- Create: `crates/capture/src/quiesce.rs`
- Test: inline module

**Interfaces:**
- Consumes: `Grid`, `Frame` (defined here), `CellRun`/`encode_grid`.
- Produces:
  - `pub struct Frame { pub seq: u32, pub rows: u16, pub cols: u16, pub alt_screen: bool, pub cursor: Option<(u16, u16)>, pub runs: Vec<CellRun>, pub changed_rows: Vec<u16>, pub volatile: Vec<Span> }`
  - `pub struct Span { pub row: u16, pub col: u16, pub len: u16, pub reason: MaskReason }`
  - `pub enum MaskReason { NoSemanticChange, KnownGlyph }`
  - `pub struct Quiescer { pub gap: std::time::Duration }`
  - `impl Quiescer { pub fn new(gap: Duration) -> Self; pub fn observe(&mut self, now: Instant, bytes_read: usize) -> bool; pub fn take_seq(&mut self) -> u32 }`
  - `pub fn changed_rows(prev: &Frame, next: &Frame) -> Vec<u16>` — rows whose runs differ; the first frame lists every row.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_single_burst_settles_once_not_once_per_byte() {
    let mut q = Quiescer::new(Duration::from_millis(150));
    let t0 = Instant::now();
    for i in 0..50 {                       // 50 bytes in 5 ms: one burst
        assert!(!q.observe(t0 + Duration::from_millis(i / 10), 1));
    }
    assert!(q.observe(t0 + Duration::from_millis(200), 0));
}

#[test]
fn quiescence_requires_the_full_gap() {
    let mut q = Quiescer::new(Duration::from_millis(150));
    let t0 = Instant::now();
    q.observe(t0, 10);
    assert!(!q.observe(t0 + Duration::from_millis(149), 0));
    assert!(q.observe(t0 + Duration::from_millis(150), 0));
}

#[test]
fn sequence_numbers_increment_per_emitted_frame() {
    let mut q = Quiescer::new(Duration::from_millis(150));
    assert_eq!(q.take_seq(), 0);
    assert_eq!(q.take_seq(), 1);
}

#[test]
fn changed_rows_reports_only_rows_that_differ() {
    let a = frame(2, 4, &[vec!['a'; 4], vec!['b'; 4]]);
    let b = frame(2, 4, &[vec!['a'; 4], vec!['c'; 4]]);
    assert_eq!(changed_rows(&a, &b), vec![1]);
}

#[test]
fn first_frame_reports_every_row_as_changed() {
    let a = frame(3, 2, &[vec![' '; 2]; 3]);
    assert_eq!(changed_rows(&Frame::empty(3, 2), &a), vec![0, 1, 2]);
}
```

Also pin Review Focus 3 and 4 here: a test that a frame captured while `alt_screen` is true is marked `alt_screen: true` and that a scroll (content shifting up one row) reports **all** rows as changed rather than none:

```rust
#[test]
fn a_scrolled_frame_is_not_reported_as_unchanged() {
    let a = frame(2, 3, &[vec!['a'; 3], vec!['b'; 3]]);
    let b = frame(2, 3, &[vec!['b'; 3], vec!['c'; 3]]);
    assert_eq!(changed_rows(&a, &b), vec![0, 1]);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ui-lm-capture quiesce`
Expected: FAIL — `Quiescer` not found.

- [ ] **Step 3: Implement `quiesce.rs`**

`observe` records `now` whenever `bytes_read > 0`, and returns true only when `bytes_read == 0` and `now - last_bytes >= gap`. `take_seq` is a counter starting at 0. `changed_rows` compares `runs` row by row (a row's runs are those whose `row` equals it) and returns differing row indices in ascending order; `Frame::empty` yields a frame whose runs vector is empty, so every non-empty row differs.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p ui-lm-capture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/capture
git commit -m "feat(capture): 150ms quiescence, frame emission, changed-row diffs"
```

---

### Task 6: Volatile-span masking

**Files:**
- Create: `crates/capture/src/mask.rs`
- Test: inline module

**Interfaces:**
- Consumes: `Frame`, `Span`, `MaskReason`.
- Produces: `pub fn classify(prev: &Frame, next: &Frame, sem: Semantics) -> Vec<Span>` where

```rust
pub struct Semantics {
    /// The application's own event stream grew between these two frames.
    pub event_between: bool,
    /// A keystroke was written to the PTY between these two frames.
    pub keystroke_between: bool,
}
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn spinner_frames_with_no_event_are_masked() {
    let a = frame(1, 8, &[vec!['.', ' ', 'r', 'u', 'n', 'n', 'i', 'n']]);
    let b = frame(1, 8, &[vec!['*', ' ', 'r', 'u', 'n', 'n', 'i', 'n']]);
    let spans = classify(&a, &b, Semantics { event_between: false, keystroke_between: false });
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].reason, MaskReason::NoSemanticChange);
    assert_eq!((spans[0].col, spans[0].len), (0, 1));
}

#[test]
fn keystroke_echo_is_not_masked() {
    let a = frame(1, 8, &[vec![' '; 8]]);
    let b = frame(1, 8, &[vec!['h', 'i', ' ', ' ', ' ', ' ', ' ', ' ']]);
    let spans = classify(&a, &b, Semantics { event_between: false, keystroke_between: true });
    assert!(spans.is_empty(), "typeahead echo is real content, not noise");
}

#[test]
fn change_with_an_event_is_not_masked() {
    let a = frame(1, 4, &[vec!['a'; 4]]);
    let b = frame(1, 4, &[vec!['b'; 4]]);
    let spans = classify(&a, &b, Semantics { event_between: true, keystroke_between: false });
    assert!(spans.is_empty());
}

#[test]
fn unchanged_frames_produce_no_spans() {
    let a = frame(1, 4, &[vec!['a'; 4]]);
    assert!(classify(&a, &a, Semantics { event_between: false, keystroke_between: false }).is_empty());
}

#[test]
fn identical_frames_capture_the_rule_that_would_have_masked_real_content() {
    // Regression: if classification compared only *whether* rows changed rather than
    // cell values, a frame pair differing solely in a wide-glyph tail would be masked.
    let mut a = frame(1, 3, &[vec!['\u{4f60}', ' ', 'x']]);
    let mut b = a.clone();
    a.runs[0].ch = '\u{4f60}';
    b.runs[0].ch = '\u{4f60}';
    assert!(classify(&a, &b, Semantics { event_between: false, keystroke_between: false }).is_empty());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ui-lm-capture mask`
Expected: FAIL — `classify` not found.

- [ ] **Step 3: Implement `classify`**

If `event_between || keystroke_between`, return empty (the change is meaningful). Otherwise diff cell by cell and emit one maximal `Span` per contiguous run of differing cells on a row, with `MaskReason::NoSemanticChange`. Record spans; do not remove the cells — the frame keeps its content and the mask is metadata (Global Constraint: masked and recorded, never dropped).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p ui-lm-capture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/capture
git commit -m "feat(capture): classify volatile spans from event and keystroke activity"
```

---

### Task 7: Session record and JSONL I/O

**Files:**
- Create: `crates/frames/src/session.rs`
- Modify: `crates/frames/src/lib.rs`
- Test: inline module

**Interfaces:**
- Consumes: `Frame`.
- Produces:
  - `pub struct SessionRecord { pub version: u32, pub started_at: String, pub ended_at: Option<String>, pub command: String, pub args: Vec<String>, pub rows: u16, pub cols: u16, pub env: Vec<(String, String)>, pub app_version: Option<String>, pub cost: Option<Cost> }`
  - `pub struct Cost { pub input_tokens: u64, pub output_tokens: u64, pub usd: Option<f64> }`
  - `pub struct SessionWriter { /* file }` with `pub fn append(&mut self, f: &Frame) -> io::Result<()>` and `pub fn finish(self, cost: Option<Cost>) -> io::Result<SessionRecord>`
  - `pub fn read_frames(dir: &Path) -> io::Result<(SessionRecord, Vec<Frame>)>`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn session_roundtrips_frames_and_cost() {
    let dir = tempdir();
    let mut w = SessionWriter::new(&dir, rec()).unwrap();
    w.append(&frame(1, 2, &[vec!['a', 'b']])).unwrap();
    w.append(&frame(1, 2, &[vec!['c', 'd']])).unwrap();
    let rec = w.finish(Some(Cost { input_tokens: 10, output_tokens: 5, usd: Some(0.01) })).unwrap();
    assert_eq!(rec.cost.unwrap().output_tokens, 5);
    let (back, frames) = read_frames(&dir).unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[1].seq, 1);
    assert_eq!(back.command, "grok");
}

#[test]
fn session_file_is_one_json_object_per_line() {
    let dir = tempdir();
    let mut w = SessionWriter::new(&dir, rec()).unwrap();
    w.append(&frame(1, 1, &[vec!['a']])).unwrap();
    w.finish(None).unwrap();
    let text = std::fs::read_to_string(dir.join("frames.jsonl")).unwrap();
    assert_eq!(text.lines().count(), 1);
    serde_json::from_str::<Frame>(text.lines().next().unwrap()).unwrap();
}

#[test]
fn a_truncated_final_line_is_skipped_rather_than_failing_the_read() {
    // Capture can be killed by Ctrl-C mid-write; a partial last line must not make
    // an otherwise good session unreadable.
    let dir = tempdir();
    let mut w = SessionWriter::new(&dir, rec()).unwrap();
    w.append(&frame(1, 1, &[vec!['a']])).unwrap();
    w.finish(None).unwrap();
    let mut f = std::fs::OpenOptions::new().append(true).open(dir.join("frames.jsonl")).unwrap();
    use std::io::Write;
    f.write_all(b"{\"seq\":1,\"rows\":1").unwrap();
    let (_, frames) = read_frames(&dir).unwrap();
    assert_eq!(frames.len(), 1);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ui-lm-frames session`
Expected: FAIL — `SessionWriter` not found.

- [ ] **Step 3: Implement `session.rs`**

`SessionWriter::new` creates `session.json` and `frames.jsonl` under the directory, writing the record on `finish` with `ended_at` filled and the frames flushed. `read_frames` parses `session.json`, then reads `frames.jsonl` line by line, keeping only successfully parsed lines and warning to stderr for each skipped one.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p ui-lm-frames`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/frames
git commit -m "feat(frames): session record with jsonl frames and cost logging"
```

---

### Task 8: Capture CLI

**Files:**
- Modify: `crates/capture/src/main.rs`
- Test: `crates/capture/tests/cli.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: binary `ui-lm-capture record --rows <n> --cols <n> --out <dir> -- <command> [args...]`.

- [ ] **Step 1: Write the failing test**

End-to-end, against a deterministic fixture program (`cmd /c`/`sh -c` printing three lines with a 200 ms pause between them), asserting that at least three frames were written, that `changed_rows` on the second frame is non-empty and smaller than the first frame's, and that `session.json` exists with `cost: null`:

```rust
#[test]
fn record_writes_frames_changed_rows_and_session() {
    let out = tempdir();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_ui-lm-capture"))
        .args(["record", "--rows", "10", "--cols", "40", "--out"])
        .arg(&out)
        .arg("--")
        .args(script_args())
        .status().unwrap();
    assert!(status.success());
    let (rec, frames) = read_frames(&out).unwrap();
    assert!(frames.len() >= 3, "expected one frame per settled line, got {}", frames.len());
    assert_eq!(frames[0].changed_rows, (0..10).collect::<Vec<_>>());
    assert!(!frames[1].changed_rows.is_empty());
    assert!(frames[1].changed_rows.len() < 10);
    assert!(rec.cost.is_none());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ui-lm-capture --test cli`
Expected: FAIL — binary has no `record` subcommand.

- [ ] **Step 3: Implement `main.rs`**

Clap derive with one subcommand, `record`. Loop: read into a 64 KiB buffer (non-blocking); feed the parser; feed the `Quiescer`; when it settles, snapshot `grid_from_screen`, classify volatile spans against the previous frame, compute `changed_rows`, assign `seq`, append. Write a keystroke flag when input is forwarded (this plan records no input; the flag stays false). On child exit, `finish(None)`. On Ctrl-C, `finish(None)` too — never leave an unflushed session.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p ui-lm-capture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/capture
git commit -m "feat(capture): record subcommand writing deterministic sessions"
```

---

### Task 9: K5 validator against the application's draw buffer

**Files:**
- Create: `crates/validate/src/compare.rs`, modify `crates/validate/src/main.rs`
- Test: inline module
- Create: `scripts/k5-dump.md` (the throwaway fork recipe, so it can be repeated and then deleted)

**Interfaces:**
- Consumes: `Frame`, `read_frames`.
- Produces:
  - `pub fn compare_frames(ours: &Frame, theirs: &Frame, ignore: &[Span]) -> Comparison`
  - `pub struct Comparison { pub total_cells: u64, pub mismatched: u64, pub first_mismatch: Option<(u16, u16, char, char)> }`
  - CLI `ui-lm-validate compare --ours <dir> --theirs <dir>` printing a mismatch count and exiting non-zero when `mismatched > 0`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn identical_frames_match() {
    let f = frame(1, 2, &[vec!['a', 'b']]);
    let c = compare_frames(&f, &f, &[]);
    assert_eq!(c.mismatched, 0);
    assert_eq!(c.total_cells, 2);
}

#[test]
fn a_single_wrong_cell_is_reported_with_its_position() {
    let a = frame(1, 2, &[vec!['a', 'b']]);
    let b = frame(1, 2, &[vec!['a', 'x']]);
    let c = compare_frames(&a, &b, &[]);
    assert_eq!(c.mismatched, 1);
    assert_eq!(c.first_mismatch, Some((0, 1, 'b', 'x')));
}

#[test]
fn masked_spans_are_excluded_from_the_count() {
    let a = frame(1, 2, &[vec!['a', 'b']]);
    let b = frame(1, 2, &[vec!['a', 'x']]);
    let ignore = vec![Span { row: 0, col: 1, len: 1, reason: MaskReason::NoSemanticChange }];
    assert_eq!(compare_frames(&a, &b, &ignore).mismatched, 0);
}

#[test]
fn style_differences_count_as_mismatches() {
    let mut a = frame(1, 1, &[vec!['a']]);
    let mut b = frame(1, 1, &[vec!['a']]);
    b.runs[0].style.bold = !a.runs[0].style.bold;
    a.runs[0].style.bold = false;
    assert_eq!(compare_frames(&a, &b, &[]).mismatched, 1);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p ui-lm-validate`
Expected: FAIL.

- [ ] **Step 3: Implement comparison, then write the fork recipe**

`compare_frames` decodes both grids and compares cell by cell, skipping cells inside `ignore` spans, counting total cells as `rows × cols`. `scripts/k5-dump.md` documents: branch `xai-org/grok-build`, add a feature `frame-dump` that writes `{"seq":n,"rows":r,"cols":c,"cells":[...]}` to a file on every draw completion, build, run the same scripted session against it with identical rows/cols/env, and record the commands used. **The fork is deleted after the comparison** — it is scaffolding, not a dependency.

- [ ] **Step 4: Run the real comparison and record the result**

Run: `ui-lm-validate compare --ours sessions/<id> --theirs sessions/<id>-buffer`
Expected: `mismatched: 0` (or a documented, explained residual). Append the outcome to the session note. **K5 is cleared only when this passes on at least three sessions.**

- [ ] **Step 5: Commit**

```bash
git add crates/validate scripts/k5-dump.md
git commit -m "feat(validate): K5 draw-buffer comparison and fork recipe"
```

---

### Task 10: Capture the W1 sessions

**Files:**
- Create: `scripts/capture-w1.ps1`
- Modify: `Notes/2026-10-27-ui-lm-grilling.md` (append the W1 result)

**Interfaces:**
- Consumes: the `record` binary.
- Produces: `sessions/w1/<n>/` directories, each with `session.json` and `frames.jsonl`; a `sessions/w1/index.json` listing session id, frame count, unique-screen count, and cost.

- [ ] **Step 1: Write the capture script**

A PowerShell script that, for each scripted scenario (startup, plain prompt, one tool call, one diff, one plan approval, one permission card, one overlay open/close), spawns `grok` via the rig at `--rows 50 --cols 160`, drives the keystrokes through the rig's input path, waits for quiescence, and stops after the scenario's terminal condition or 120 s. It writes `index.json`.

- [ ] **Step 2: Run it and check the artefacts**

Run: `pwsh scripts/capture-w1.ps1`
Expected: ≥30 sessions, every one readable by `read_frames`, `index.json` populated, and the total cost line reported.

- [ ] **Step 3: Verify K5 across the captured sessions**

Run: `ui-lm-validate compare` against the three sessions replayed through the patched fork.
Expected: zero mismatches, recorded in the session note.

- [ ] **Step 4: Report cost against the sanctioned budget**

Sum `cost` across all sessions and write the figure into the note, against the ~$1,500 credit (C15).

- [ ] **Step 5: Commit**

```bash
git add scripts/capture-w1.ps1 sessions/w1/index.json
git commit -m "feat: capture W1 grok sessions with validated frames"
```

Session recordings themselves stay out of git (`.gitignore`); only the index and the script are committed.

---

## Self-Review

**Spec coverage.** §7.1 capture rig — Tasks 1–8. §7.1 emulator validation (K5) — Task 9. §8 W1 row, including "enumerate Grok Build's screens" — Task 10 produces the raw material; enumeration itself is the first task of the next plan. §4 C15 cost logging — Tasks 7 and 10. §4 C17 (record latency, never optimise) — respected: the rig records frame timestamps implicitly through `seq` and session timing, and contains no optimisation work. §7.2–§7.5 are out of scope by design.

**Gaps found and fixed.** The spec's "changed-rows-only input is mandatory" had no task that produced the diff — Task 5 added `changed_rows` and Task 8 proved it end-to-end. The spec's volatile-span requirement had no definition of "volatile" — Task 6 adds the event/keystroke rule, which is the only definition that survives typeahead echo.

**Type consistency.** `Frame`, `Span`, `MaskReason`, `CellRun`, `Style`, `StyledCell`, `Grid`, `changed_rows`, `grid_from_screen`, `SessionWriter`, `Cost` are used with the same names and shapes in every task that touches them. `Semantics` is introduced in Task 6 and used only there.

**Review Focus.** Full-width characters → Task 3. Trailing whitespace and tabs → Tasks 2 and 3. Alt-screen and mid-scroll frames → Task 5. Typeahead bursts → Tasks 5 and 6. Ambiguous widths → Task 3, by refusing to re-derive width from `char`.

**Proportion.** Ten tasks against a 60-line spec section. Code blocks are test bodies and struct signatures, not implementations.
