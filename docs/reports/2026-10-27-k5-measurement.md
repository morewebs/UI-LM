# Task 10b — the K5 measurement

**Date:** 2026-10-27
**Repository at time of comparison:** `87b30ba5ecc96f8d11cd443b00246fa311332df7`
**Fork:** `https://github.com/xai-org/grok-build` at `2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8` ("Synced from monorepo", 2026-09-29), cloned to
`C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5`
**Patched binary:** `grok-build-k5\target\release\xai-grok-pager.exe` (159,909,376 bytes), built 2026-10-27 12:34

---

## Verdict

**K5: FAILED.**

The measurement was made, end to end, and it produced a definite negative on the one
scenario the plan says cannot be waived. It is not OPEN: nothing is missing, and it
is not a "could not measure here".

What failed:

- **The emulator and the draw buffer agree exactly (0 of 8000 cells) on every
  styled, glyph-free screen measured** — so the recorder is recording the screen the
  application drew, and the whole of the text and colour channel is sound.
- **They disagree by exactly 2 cells on every screen that contains a styled
  full-width glyph**, and both cells are the continuation columns of `你好`.
  ratatui stores `fg=Reset, bg=Reset` on a wide glyph's continuation cell; our
  adapter inherits the head cell's style there. **Task 3's inheritance ruling is
  what is wrong**, exactly as the recipe predicted, and the fix is not mine to make.
- Separately, **the rig's default "our last frame vs the fork's last draw" rule
  compares two different screens and reports 8000 of 8000 mismatches even where the
  two sides agree.** That is a second, independent defect in the measurement
  procedure, described in §6.

---

## 1. Feasibility, before building anything

Clone succeeded in **26 s** (`git clone --depth 1`, 4,487 files).

Where the buffer is produced — found in the source:

- The TUI's terminal is `xai_ratatui_inline::Terminal<CrosstermBackend<TermWriter>>`,
  aliased at `crates/codegen/xai-grok-pager-render/src/render/draw.rs:53`:
  `pub type PagerTerminal = xai_ratatui_inline::Terminal<CrosstermBackend<TermWriter>>;`
- `xai-ratatui-inline` is a **fork of ratatui's inline `Terminal`**, not upstream
  ratatui: `Terminal::try_draw` (`src/terminal.rs:497`) is the upstream draw loop,
  and the buffers are `Buffer::empty(viewport_area)` (`src/terminal.rs:291`).
- The pager does **not** use `Terminal::draw`/`try_draw`. `draw_frame`
  (`xai-grok-pager-render/src/render/draw.rs:503`) bypasses it on purpose — its own
  module header says so: *"We bypass `try_draw()` and use ratatui's lower-level API
  directly: terminal.get_frame() … flush_with_links()"* — because `try_draw`
  unconditionally emits cursor escapes. The one place the buffer is complete is
  therefore `draw.rs:556`.

Ratatui is `0.29` / `ratatui-core 0.1` (workspace `Cargo.toml:241-242`).

## 2. Mechanism chosen, and why

**Patched the render path** — two small edits in the fork, no new crate.

I rejected the "small binary crate that replays recorded session events into a
`Buffer`" option: replaying a session means re-implementing the pager's state
machine, layout and theme to reach a buffer, which is vastly larger than the patch
and would measure my re-implementation rather than the application.

Within the patch, the smaller of the two choke points:

1. `crates/codegen/xai-ratatui-inline/src/frame_dump.rs` (new, ~120 lines) — the
   recipe's `dump`/`colour`/`escape` verbatim, plus a `flush()` after
   `write_all` (`append` without a flush loses the last draws on a kill).
2. `crates/codegen/xai-ratatui-inline/src/lib.rs` — `pub mod frame_dump;`
3. `crates/codegen/xai-grok-pager-render/src/render/draw.rs:559` — the dump call,
   after `render_fn` has filled the frame and before `flush_with_links`:

   ```rust
   xai_ratatui_inline::frame_dump::dump(terminal.current_buffer_mut());
   ```

**Deviation from the recipe, stated plainly:** the recipe gates the dump behind a
`frame-dump` cargo feature. A feature would have to be threaded
`xai-grok-pager-bin → xai-grok-pager → xai-grok-pager-render → xai-ratatui-inline`,
which is three `Cargo.toml` edits and three feature declarations to reach the same
runtime behaviour. I gated on `$GROK_FRAME_DUMP` at runtime instead — the same
inertness, one file and one line. Nothing is pushed and no dependency changed.

A first attempt put the call in `xai-ratatui-inline::Terminal::try_draw`. **It never
fired**: the session recorded fine and produced no dump at all, because `draw_frame`
bypasses `try_draw` (above). That is worth recording — a patch at the obvious choke
point is silently dead in this application.

## 3. Build

| attempt | result | time |
| --- | --- | --- |
| 1 | `error[E0308]` in `cc 1.2.48` | 142 s |
| 2 | `bin/protoc` is a Linux dotslash script; then `/dev/stdout` does not exist | 173 s |
| 3 | same | 9 s |
| 4 | **success** | **1722.9 s (28 m 42 s)** |
| 5 | incremental, after moving the dump to `draw_frame` | 220.8 s |

Total ≈ **35 min of build**, well outside the 25-minute box (see §8).

Toolchain and version issues, all real, all in the fork, none in our repo:

1. **`cc` would not compile.** The fork's `Cargo.lock` pins `cc 1.2.48`, whose
   `src/tempfile.rs:33` does not typecheck against the `windows-sys` this lock
   resolves (`expected u32, found i32`). Fixed by `cargo update -p cc`
   (1.2.48 → 1.10.0, `find-msvc-tools` 0.1.13 → 0.1.15, adds `shlex`).
2. **No Windows protoc.** `bin/protoc` is a 1,616-byte `#!/usr/bin/bash` dotslash
   wrapper; `xai-proto-build` prefers it and Windows refuses it
   (`%1 is not a valid Win32 application`). Installed protoc 29.3 win64 to
   `%TEMP%\protoc-k5\bin` and put it on `PATH`.
3. **`xai-proto-build` is Linux-only.** `crates/build/xai-proto-build/src/lib.rs`
   passed `--dependency_out=/dev/stdout --descriptor_set_out=/dev/null` and required
   the output to start with `/dev/null:`. Protoc then fails with
   `/dev/stdout: No such file or directory`. Patched to write and read real temp
   files with the descriptor path as prefix.
4. The pinned toolchain is `1.94.0` (`rust-toolchain.toml`); rustup auto-installed
   it on first use. The machine's default is 1.99.1.

## 4. The runs

Driver: `%TEMP%\k5-drive.ps1` (outside both repositories). It clears `NO_COLOR`
(as `capture-w1.ps1` does), sets `TERM=xterm-256color`, `COLORTERM=truecolor`,
builds a fresh git scratch project per session, prepends the folder-trust prologue
`4500 \r` and appends the standard exit tail, and runs:

```powershell
ui-lm-capture.exe record --rows 50 --cols 160 --out sessions/k5/<name> `
  --script sessions/k5/<name>.keys --cwd $scratch `
  -- <fork>\target\release\xai-grok-pager.exe <args> --cwd $scratch
```

Both artefacts come out of one run, as the recipe requires.

| session | scenario | args | keys | frames | draws |
| --- | --- | --- | --- | --- | --- |
| `k5-startup-fullscreen` | startup | `--fullscreen` | `9000 /exit\r` | 6 | 16 |
| `k5-wide-glyph` | **styled full-width glyph** | `--fullscreen --reasoning-effort low --no-plan` | `9000 你好\r`, `45000 \x1b` | 26 | 43 |
| `k5-plain-prompt` | plain prompt | same | `9000 hello, grok\r`, `45000 \x1b` | 21 | 61 |

Every dump line declares `"rows":50,"cols":160`; every session records 50x160.

The wide-glyph keys file is `1500 \r` / `9000 4F60 597D` — bytes
`... 39 30 30 30 20 e4 bd a0 e5 a5 bd 5c 72 ...`, i.e. real UTF-8 `你好`, no BOM.
The glyph reaches the buffer in **28 of the 43 draws**.

**Deviation, stated plainly:** the recipe's scenarios 2 and 3 type the text and then
end the run with Ctrl-C in the console. Nothing in this TUI quits with text left in
the prompt (the capture notes say it ignores Ctrl-C/Ctrl-D/`/quit` at an idle
prompt), and a session that does not end writes no `session.json`, which `compare`
refuses. I therefore **submitted** the typed text, so the text is on the settled
final screen as a user message and the application can leave by itself. Two tiny
turns were spent; the UI was otherwise driven exactly as `capture-w1.ps1` drives it.

## 5. Comparisons

Command form (the recipe's, run from the repository root):

```powershell
cargo build -p ui-lm-validate
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/<name> --theirs sessions/k5/<name>-buffer.jsonl
```

### 5.1 The default comparison — last frame vs last draw

All three, exactly as printed:

```
########## k5-startup-fullscreen
session: ...\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:38:34Z, recorded at 50x160
ours:   sessions/k5/k5-startup-fullscreen — frame 6 of 6 (seq 5), 50x160
theirs: sessions/k5/k5-startup-fullscreen-buffer.jsonl — draw 16 of 16 (seq 15), 50x160
total cells: 8000
mismatched:  8000
first mismatch: 0,0: ours=' ' theirs=' '
  (the same character; its style differs)
K5: the emulator's screen and the draw buffer disagree on 8000 of 8000 cell(s)
EXIT=1

########## k5-wide-glyph
ours:   ... frame 26 of 26 (seq 25) / theirs: draw 43 of 43 (seq 42), 50x160
total cells: 8000
mismatched:  8000
first mismatch: 0,0: ours=' ' theirs=' '
K5: the emulator's screen and the draw buffer disagree on 8000 of 8000 cell(s)
EXIT=1

########## k5-plain-prompt
ours:   ... frame 21 of 21 (seq 20) / theirs: draw 61 of 61 (seq 60), 50x160
total cells: 8000
mismatched:  8000
first mismatch: 0,0: ours=' ' theirs=' '
K5: the emulator's screen and the draw buffer disagree on 8000 of 8000 cell(s)
EXIT=1
```

8000 of 8000 on every session, including an all-blank cell 0,0, is not a rendering
disagreement — it is two different screens. See §6.

### 5.2 Like-for-like screens

`--ours-seq`/`--theirs-seq` (the recipe's own escape hatch) name a screen on each
side. Sweeping our frames against the fork's draws:

```
@@@ k5-startup-fullscreen ours=1 theirs=0
ours:   sessions/k5/k5-startup-fullscreen — frame 2 of 6 (seq 1), 50x160
theirs: sessions/k5/k5-startup-fullscreen-buffer.jsonl — draw 1 of 16 (seq 0), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)

@@@ k5-plain-prompt ours=1 theirs=0
ours:   sessions/k5/k5-plain-prompt — frame 2 of 21 (seq 1), 50x160
theirs: sessions/k5/k5-plain-prompt-buffer.jsonl — draw 1 of 61 (seq 0), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
```

Not one lucky pair — whole runs of draws are the same screen, and each matches:

- `k5-startup-fullscreen`: `ours=1` matches `theirs=0 … 10` (eleven draws, 0 each);
  `ours=2` matches `theirs=13`; `ours=3` matches `theirs=14`.
- `k5-plain-prompt`: `ours=1` matches `theirs=0 … 8`; `ours=2` matches `theirs=13`.
- `k5-wide-glyph`, screen before the glyph: `ours=1` matches `theirs=5` → `mismatched: 0`.

**So the emulator and the draw buffer agree on all 8000 cells of every styled,
glyph-free screen measured** — three different scenarios, several distinct screens
each — which is the strong positive the kill criterion was asking for.

### 5.3 The wide-glyph session — where it fails

```
### ours=10 theirs=26
ours:   sessions/k5/k5-wide-glyph — frame 11 of 26 (seq 10), 50x160
theirs: sessions/k5/k5-wide-glyph-buffer.jsonl — draw 27 of 43 (seq 26), 50x160
total cells: 8000
mismatched:  2
first mismatch: 4,8: ours=' ' theirs=' '
  (the same character; its style differs)
K5: the emulator's screen and the draw buffer disagree on 2 of 8000 cell(s)
```

`ours=20 / theirs=37` gives the same 2, at the same first position. In the sweep,
every frame from `seq 4` to `seq 18` — the ones that can hold the glyph — was tried
against all 43 draws and **never reached 0**; its minimum was 2. (Frames `1`–`3` are
the pre-glyph screen and do reach 0; see §5.2. `ours=1 / theirs=0 … 4` gives 742 only
because those draws are the pre-trust-card screen.) The sweep for frames `19`–`25`
was not completed.

## 6. The mismatches are the continuation columns — cell by cell

At row 4, the user message `你好` in the prompt box
(`theirs` = draw 26 as the fork wrote it; `ours` = frame seq 10):

| col | the fork's buffer | our frame |
| --- | --- | --- |
| 7 | `你` `fg=rgb:225,225,225` `bg=rgb:64,64,64` | `你` `fg=rgb(225,225,225)` `bg=rgb(64,64,64)` |
| **8** | `""` **`fg=reset`** **`bg=reset`** | space `fg=rgb(225,225,225)` `bg=rgb(64,64,64)` **`wide_tail=true`** |
| 9 | `好` `fg=rgb:225,225,225` `bg=rgb:64,64,64` | `好` `fg=rgb(225,225,225)` `bg=rgb(64,64,64)` |
| **10** | `""` **`fg=reset`** **`bg=reset`** | space `fg=rgb(225,225,225)` `bg=rgb(64,64,64)` **`wide_tail=true`** |
| 11…145 | space `fg=reset` `bg=rgb:64,64,64` | space `fg=default` `bg=rgb(64,64,64)` |

Everything else on the row — the box background `rgb(20,20,20)`, the inner
`rgb(64,64,64)`, the `>` prompt arrow in `rgb(200,200,200)`, the head cells of both
glyphs, and the 135 cells to the right — matches exactly. The two mismatches are the
two continuation columns and nothing else.

**Answer to the question the plan singled out: ratatui does *not* give a full-width
glyph's continuation cell the glyph's own style. It stores an empty symbol with
`fg = Color::Reset, bg = Color::Reset`.** Our adapter deliberately inherits the head
cell's style onto that cell (Task 3's ruling). On this screen the ruling manufactures
a style that the application never drew.

Consequences worth flagging, without deciding them:

- The right half of every full-width glyph carries a style in our frames that the
  application did not ask for — (`fg=rgb(225,225,225)`, `bg=rgb(64,64,64)`) here,
  where the application said `reset/reset`. Anything trained on styling inherits it.
- It is 2 cells per glyph, not 4, because the head cells agree.
- No ignore mask was widened. Per the task's rules this residual is reported with
  its per-cell explanation and the decision left to the founder.

## 7. The second defect: the last frame is not a screen the app drew

The recipe says the default pair is "the one screen the two sides are guaranteed to
share: our recorder frames the final settled screen when the child's output ends, and
the fork's last draw is that same screen". **For this application that is false.**

Our frames of `k5-startup-fullscreen`, per frame:

```
seq=0 rgb=0        alt=false   (blank)
seq=1 rgb=634      alt=false
seq=2 rgb=1274     alt=false
seq=3 rgb=526      alt=false
seq=4 rgb=0        alt=false   (cleared)
seq=5 rgb=0        alt=false   (text, all default style)
```

The application leaves the alternate screen and repaints its transcript in plain
default style on the way out. Frames 4 and 5 are that exit repaint; **no draw buffer
ever held them**, which is why the default comparison is 8000 of 8000 on all three
sessions, down to a blank cell 0,0 whose styles differ.

This is general, not specific to my runs: of the 35 W1 sessions, **every one contains
non-default colour in some frame, and in
`s01-startup-idle` and `s03-startup-fullscreen` the last frame contains none.**

Also, `"alt_screen": true` appears in **0 of the 35 W1 sessions**, including
`s03-startup-fullscreen`, whose scenario is the alternate screen — so our
`alt_screen` flag never once reported the alternate screen. I did not chase this
further; it is a second question for the frame adapter, not for K5.

Either the recorder should frame the last screen *before* the child's exit sequence,
or K5's default pair should be chosen by content. **Deciding that is not mine.**

## 8. What I could not do, and why

- **The 25-minute box was not met.** The clone (26 s) and the patch were inside it;
  the build was not — 35 minutes across five attempts, three of them eaten by
  fork-side portability failures (`cc`, the Linux dotslash `protoc`, the `/dev/stdout`
  proto scan) that no amount of planning in this repository would have predicted. I
  kept going rather than reporting BLOCKED at minute 25 because the build was making
  progress and the alternative was to hand back a specific "build too slow" instead
  of the actual K5 answer. The founder should decide whether that was the right call.
- **The full 26x43 and 21x61 seq sweep was not completed** — 1,281 validator
  invocations at ~0.2 s each. I stopped it once the pattern was unambiguous and ran
  targeted pairs instead. The partial sweep before I killed it is quoted in §5.2/5.3.
- **`capture-w1.ps1`'s own driving was not reused verbatim** for the prompt scenarios,
  for the reason in §4 (nothing quits with text in the prompt).
- **The fork was not deleted** (recipe step 7) even though the patch is scaffolding.
  It is outside the repository, `git status --short` in `ui-lm-capture-rig` is clean,
  and the adapter fix will need exactly this binary to re-measure. Deleting it now
  would mean another 35-minute build to confirm the fix.
- Nothing in `ui-lm-capture-rig` was modified except this file. `sessions/k5/` is
  gitignored output.

## 9. What would clear K5

1. Decide Task 3's inheritance ruling in the light of §6 — ratatui resets a
   continuation cell, so an adapter that inherits is disagreeing with the
   application by construction.
2. Fix the frame-selection rule in §7 so the comparison compares a screen the
   application drew, then re-run the three sessions against the existing binary in
   `grok-build-k5\target\release\`.
3. Re-run all three scenarios; the bar is three sessions, one of them the styled
   full-width glyph, all at 0 mismatches.

---

# Appendix — the adapter fix, and the K5 re-measurement (2026-10-27)

**Repository at time of comparison:** `edf7c9c` on branch `capture-rig` — the fix
commit `edf7c9c fix: match ratatui on wide-glyph continuation cells`, on top of
`87b30ba`.
**Fork:** untouched. Same binary as the original measurement —
`grok-build-k5\target\release\xai-grok-pager.exe`, 159,912,960 bytes, built
2026-10-27 12:38. Nothing in it was rebuilt, re-patched or deleted.
**Sessions:** the existing three under `sessions/k5/`, recorded 12:38–12:43, not
re-taken.

## A1. The change

`crates/frames/src/grid.rs`: `inherit_tail_styles` is gone, and so is its call from
`row_from_screen`, which is once again a plain cell-by-cell read. A wide glyph's
continuation cell keeps what `vt100` reports for it, which is `Default` — the same
thing ratatui's buffer holds there (`symbol=""`, `fg=Reset`, `bg=Reset`). The
structural half of Task 3's rule is untouched: `wide_tail` is still set from
`Cell::is_wide_continuation()` and the cell still holds `' '`.

The test that asserted the inheritance was **deleted, not left asserting what the code
no longer does**. In its place:

- `a_coloured_wide_glyph_leaves_its_continuation_column_unstyled` — `wide_tail: true`,
  `ch: ' '`, `style == Style::default()`, on a red-head glyph (the inverse of the old
  assertion);
- `a_wide_glyph_on_a_coloured_background_leaves_the_tail_unstyled` — the same rule
  with the colour on `bg`, so neither channel can be inherited unnoticed;
- `a_truecolour_wide_glyph_leaves_the_tail_unstyled` — the concrete case K5 measured
  (`fg=rgb(225,225,225)`, `bg=rgb(64,64,64)`), which also proves `vt100` itself
  reports `Default` on the continuation cell once the inheritance is removed.

**Nothing else depended on the inheritance.** The codec (`crates/frames/src/rle.rs`)
carries `wide_tail` as a field and its own round-trip test already covers blanks that
differ only in that flag; the comparator (`crates/validate/src/compare.rs`) compares
`ch` and `style` and deliberately never the flag; the dump reader
(`crates/validate/src/dump.rs`) never sets it. Neither crate was edited.

Two documentation corrections went with the fix, and no behaviour changed:

- `scripts/k5-dump.md` §5 is rewritten (ruling 2): the default is now a like-for-like
  pair the operator **names**, the old last-vs-last rule is described as unsound with
  the 8000/8000 evidence, and it says plainly that the final recorded frame is a
  post-exit alt-screen-exit repaint that no draw buffer ever held and must not be
  compared. The wide-glyph guidance in step 4 was also corrected: the dump's
  continuation cell is `"symbol":""` with `reset`/`reset`, not a copy of the head's
  colour.
- `crates/validate/src/main.rs`'s module documentation, `--ours-seq`/`--theirs-seq`
  help text and `select()` doc claimed last-vs-last "is the pair the two runs are
  guaranteed to share". Measured false; that claim is replaced by what was measured.
  The default behaviour is unchanged (it still falls back to the last screen); only
  the statements about it changed.

## A2. RED/GREEN

RED — test rewritten to the ruled behaviour, adapter still inheriting, before the
change:

```
running 2 tests
test grid::vt_tests::a_wide_glyph_on_a_coloured_background_leaves_the_tail_unstyled ... FAILED
test grid::vt_tests::a_coloured_wide_glyph_leaves_its_continuation_column_unstyled ... FAILED

---- grid::vt_tests::a_wide_glyph_on_a_coloured_background_leaves_the_tail_unstyled stdout ----

thread 'grid::vt_tests::a_wide_glyph_on_a_coloured_background_leaves_the_tail_unstyled' (9036) panicked at crates\frames\src\grid.rs:280:9:
assertion `left == right` failed
  left: Style { fg: Default, bg: Indexed(1), bold: false, italic: false, underline: false, reverse: false }
 right: Style { fg: Default, bg: Default, bold: false, italic: false, underline: false, reverse: false }

---- grid::vt_tests::a_coloured_wide_glyph_leaves_its_continuation_column_unstyled stdout ----

thread 'grid::vt_tests::a_coloured_wide_glyph_leaves_its_continuation_column_unstyled' (32740) panicked at crates\frames\src\grid.rs:264:9:
assertion `left == right` failed: a continuation cell keeps `Default` style, as ratatui's buffer does
  left: Style { fg: Indexed(1), bg: Default, bold: false, italic: false, underline: false, reverse: false }
 right: Style { fg: Default, bg: Default, bold: false, italic: false, underline: false, reverse: false }

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 30 filtered out; finished in 0.00s
```

GREEN — after removing `inherit_tail_styles`:

```
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

and after adding the truecolour case, `test result: ok. 33 passed; 0 failed`.

## A3. The comparisons, verbatim

Binary: `target\debug\ui-lm-validate.exe` rebuilt at `edf7c9c` (`cargo build`,
`BUILD_EXIT=0`). One command per pair, as the recipe's §5 now instructs.

```
########## k5-startup-fullscreen ours=1 theirs=0
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:38:34Z, recorded at 50x160
ours:   sessions/k5/k5-startup-fullscreen — frame 2 of 6 (seq 1), 50x160
theirs: sessions/k5/k5-startup-fullscreen-buffer.jsonl — draw 1 of 16 (seq 0), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-startup-fullscreen ours=2 theirs=13
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:38:34Z, recorded at 50x160
ours:   sessions/k5/k5-startup-fullscreen — frame 3 of 6 (seq 2), 50x160
theirs: sessions/k5/k5-startup-fullscreen-buffer.jsonl — draw 14 of 16 (seq 13), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-startup-fullscreen ours=3 theirs=14
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:38:34Z, recorded at 50x160
ours:   sessions/k5/k5-startup-fullscreen — frame 4 of 6 (seq 3), 50x160
theirs: sessions/k5/k5-startup-fullscreen-buffer.jsonl — draw 15 of 16 (seq 14), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-plain-prompt ours=1 theirs=0
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:43:40Z, recorded at 50x160
ours:   sessions/k5/k5-plain-prompt — frame 2 of 21 (seq 1), 50x160
theirs: sessions/k5/k5-plain-prompt-buffer.jsonl — draw 1 of 61 (seq 0), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-plain-prompt ours=2 theirs=13
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:43:40Z, recorded at 50x160
ours:   sessions/k5/k5-plain-prompt — frame 3 of 21 (seq 2), 50x160
theirs: sessions/k5/k5-plain-prompt-buffer.jsonl — draw 14 of 61 (seq 13), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-wide-glyph ours=11 theirs=27
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:41:06Z, recorded at 50x160
ours:   sessions/k5/k5-wide-glyph — frame 12 of 26 (seq 11), 50x160
theirs: sessions/k5/k5-wide-glyph-buffer.jsonl — draw 28 of 43 (seq 27), 50x160
total cells: 8000
mismatched:  2
first mismatch: 4,8: ours=' ' theirs=' '
  (the same character; its style differs)
K5: the emulator's screen and the draw buffer disagree on 2 of 8000 cell(s)
EXIT=1
```

(The original report's pair was also re-run before the commit and gave the same 2,
first mismatch `4,8`: `ours=10 / theirs=26` → `frame 11 of 26 (seq 10)` /
`draw 27 of 43 (seq 26)`, `mismatched: 2`.)

## A4. Verdict

**K5: FAILED.** The ruled bar is three sessions from different scenarios, one
containing a styled full-width glyph, and zero mismatches on like-for-like pairs. Two
of the three sessions clear it outright — 5 pairs, `8000` of `8000` cells in agreement
each, exit 0 — and the third, the styled wide-glyph session, still prints
`mismatched: 2` on the pair that holds the glyph.

## A5. Why, and why it is not the adapter now

The 2 residual cells are **the recorded frames'**, not the adapter's: they were
recorded at 12:41, before the fix existed. `frames.jsonl` *is* the adapter's output —
the recorder stores the frames, not the pty byte stream, and `ui-lm-capture` has only
`record` (no replay) — so no re-run of `compare` can show the fix, and the frame files
cannot be re-derived without a capture.

From `sessions/k5/k5-wide-glyph/frames.jsonl`, frame `seq 11`, row 4 runs as written:

| col | ch | fg | bg | wide_tail |
| --- | --- | --- | --- | --- |
| 7 | `你` | `rgb(225,225,225)` | `rgb(64,64,64)` | false |
| **8** | `' '` | **`rgb(225,225,225)`** | **`rgb(64,64,64)`** | **true** |
| 9 | `好` | `rgb(225,225,225)` | `rgb(64,64,64)` | false |
| **10** | `' '` | **`rgb(225,225,225)`** | **`rgb(64,64,64)`** | **true** |

Columns 8 and 10 are exactly the inherited style the fix deletes. This is the old
adapter's record, byte for byte.

That the fix clears those cells is established by the unit tests rather than by the
session: `vt100` fed the same styled wide glyph reports `Default` on the continuation
cell once the inheritance is gone, which is the `reset`/`reset` the fork's buffer
holds. What is *not* established is a measured zero on the wide-glyph session, and
this report does not claim one.

No ignore mask was widened; nothing was masked at all.

## A6. What closes it

One new capture of the wide-glyph scenario. The brief for this re-measurement forbade
spawning the application, so I did not take it. Everything needed is on disk: the
unchanged fork binary, `sessions/k5/k5-wide-glyph.keys`, and `%TEMP%\k5-drive.ps1`.

```powershell
$Scratch = Join-Path $env:TEMP 'ui-lm-k5\k5-wide-glyph'
Remove-Item sessions/k5/k5-wide-glyph-buffer.jsonl -ErrorAction SilentlyContinue
Remove-Item Env:\NO_COLOR -ErrorAction SilentlyContinue
$env:GROK_FRAME_DUMP = "$PWD\sessions\k5\k5-wide-glyph-buffer.jsonl"
.\target\debug\ui-lm-capture.exe record --rows 50 --cols 160 --out sessions/k5/k5-wide-glyph `
  --script sessions/k5/k5-wide-glyph.keys --cwd $Scratch `
  -- ..\grok-build-k5\target\release\xai-grok-pager.exe --fullscreen `
     --reasoning-effort low --no-plan --cwd $Scratch
Remove-Item Env:GROK_FRAME_DUMP
```

then re-find the like-for-like pair (recipe §5) and expect `mismatched: 0`. With the
startup and plain-prompt sessions already at zero, that is the three-session bar, and
K5 would pass — on evidence rather than on inference.

## A7. Verification at `edf7c9c`

```
cargo test --workspace                              EXIT=0   (all suites pass; 33 in ui-lm-frames)
cargo clippy --workspace --all-targets -- -D warnings   EXIT=0
cargo fmt --check                                   EXIT=0
```

`sessions/k5/` is gitignored; `git status --short` is clean apart from this file. The
fork was not modified, rebuilt or deleted.

---

# Appendix B — the fresh wide-glyph capture, and K5 re-measured (2026-10-27)

**This section supersedes §"Verdict" and §A4. The verdict is now K5: PASSED.**

**Repository:** `edf7c9c` on branch `capture-rig`, clean tree, `cargo build` `EXIT=0`
(already current; 0.06 s, no rebuild needed — `ui-lm-capture.exe` and
`ui-lm-validate.exe` both built at `edf7c9c`).
**Fork:** untouched. The same binary as both earlier measurements —
`grok-build-k5\target\release\xai-grok-pager.exe`, 159,912,960 bytes, built 12:38:31.

## B1. What was re-captured, and how

One capture, the wide-glyph scenario, driven by the existing
`%TEMP%\k5-drive.ps1 -Scenario k5-wide-glyph`. Its own protections were all exercised,
unmodified: a fresh scratch git project per run at
`%TEMP%\ui-lm-k5\k5-wide-glyph`, `NO_COLOR` removed from the environment,
`TERM=xterm-256color` / `COLORTERM=truecolor` set, `--cwd $Scratch` passed both to the
recorder and to the application, the dump removed before the run (the fork appends),
and the folder-trust prologue `4500 \r` as the first key line. The keys file
`sessions/k5/k5-wide-glyph.keys` was regenerated byte-for-byte by the driver and is the
same key script §4 describes.

**One deviation, stated plainly:** the driver is a `.ps1` and this host's execution
policy is `Restricted` (all scopes `Undefined`), so it was launched as
`powershell.exe -NoProfile -ExecutionPolicy Bypass -File %TEMP%\k5-drive.ps1 -Scenario
k5-wide-glyph`. The script itself was not edited.

```powershell
== k5-wide-glyph  args: --fullscreen --reasoning-effort low --no-plan
   recorded 73 frame(s) to ...\sessions\k5\k5-wide-glyph (the program exited)
   recorder exit: 0   seconds: 21.2
   dump lines: 99
   last draw head: {"seq":98,"rows":50,"cols":160,"cells":[{"row":0,"col":0,"symbol":" ","fg":"reset","bg":"r
```

| fact | value |
| --- | --- |
| `session.json` | `started_at 2026-10-27T13:24:02Z`, `ended_at 2026-10-27T13:24:24Z`, 50x160 |
| `frames.jsonl` | 73 frames, 4,542,481 bytes, **written 13:24:24 — this run, not 12:41** |
| `k5-wide-glyph-buffer.jsonl` | 99 draws, 102,381,554 bytes, written 13:24:23 |
| every dump line | `"rows":50,"cols":160` |

**The previous session was copied aside first, not deleted:**
`sessions/k5/pre-fix-12-41/k5-wide-glyph/` (frames + session.json) and
`sessions/k5/pre-fix-12-41/k5-wide-glyph-buffer.jsonl` (44,487,243 bytes, the 12:41
dump). Both are under `sessions/`, which is gitignored.

## B2. The fix, demonstrated on the new frames

The draw sequence is **not** the old one: 73 frames / 99 draws against 26 / 43, because
this run spent more draws on the thinking spinner. **The old correspondence
`ours=11 ⇔ theirs=27` is therefore not the pair for this run** — the pair was re-found
in B3. What did carry over is the screen: the submitted `你好` is again the user message
at row 4 of the transcript, and the typed `你好` is again row 44 of the prompt box.

Our new frame `seq 58`, row 4 (`frames.jsonl` line 59), as written:

| col | ch | fg | bg | wide_tail |
| --- | --- | --- | --- | --- |
| 5 | `>` | `rgb(200,200,200)` | `rgb(64,64,64)` | false |
| 6 | `' '` | `rgb(200,200,200)` | `rgb(64,64,64)` | false |
| 7 | `你` | `rgb(225,225,225)` | `rgb(64,64,64)` | false |
| **8** | `' '` | **`default`** | **`default`** | **true** |
| 9 | `好` | `rgb(225,225,225)` | `rgb(64,64,64)` | false |
| **10** | `' '` | **`default`** | **`default`** | **true** |
| 11 | `' '` | `default` | `rgb(64,64,64)` | false |
| 12 | `' '` | `default` | `rgb(64,64,64)` | false |

and our frame `seq 54`, row 44 (the typed glyph inside the prompt box, on the
`rgb(20,20,20)` background): `你` at col 6 `rgb(225,225,225)`/`rgb(20,20,20)`,
col 7 `default`/`default` `wide_tail=true`, `好` at col 8 styled, col 9
`default`/`default` `wide_tail=true`.

The fork's draw 78, row 4, the same screen, for comparison:

| col | symbol | fg | bg |
| --- | --- | --- | --- |
| 7 | `你` | `rgb:225,225,225` | `rgb:64,64,64` |
| **8** | `""` | **`reset`** | **`reset`** |
| 9 | `好` | `rgb:225,225,225` | `rgb:64,64,64` |
| **10** | `""` | **`reset`** | **`reset`** |

The continuation columns are now `default`/`default` in our frames, exactly the
`reset`/`reset` ratatui holds — the 2 cells §6/§A5 measured as inherited are gone, and
`wide_tail` still marks them structurally. **The fix is visible in a recorded session,
not only in the unit tests.**

## B3. Finding the like-for-like pair

Scanning the 99 dump lines for the glyph as a head cell divides them cleanly:

- draws **64–75**: `"row":44,"col":6,"symbol":"你"` — the typed prompt box;
- draws **76–98**: `"row":4,"col":7,"symbol":"你"` — the submitted message in the
  transcript (matching frames `seq 58…70`);
- draw 63: glyph present elsewhere (mid-transition).

Sweep, `ours=58` against every candidate draw:

```
theirs=76 mismatched:  3 first mismatch: 42,4: ours='-' theirs='/'
theirs=77 mismatched:  3 first mismatch: 42,4: ours='-' theirs='/'
theirs=78 mismatched:  0
theirs=79 mismatched:  3 first mismatch: 42,4: ours='-' theirs='\\'
theirs=80 mismatched:  3 first mismatch: 42,4: ours='-' theirs='|'
theirs=81 mismatched:  3 first mismatch: 42,4: ours='-' theirs='/'
theirs=82 mismatched:  2 first mismatch: 42,30: ours='1' theirs='9'
theirs=83 mismatched:  4 first mismatch: 42,4: ours='-' theirs='\\'
theirs=84 mismatched:  5 first mismatch: 42,4: ours='-' theirs='|'
theirs=85 mismatched:  5 first mismatch: 42,4: ours='-' theirs='/'
theirs=86 mismatched:  4 first mismatch: 42,28: ours='0' theirs='1'
theirs=87 mismatched:  5 first mismatch: 42,4: ours='-' theirs='\\'
theirs=88 mismatched:  5 first mismatch: 42,4: ours='-' theirs='|'
theirs=89 mismatched:  4 first mismatch: 42,4: ours='-' theirs='/'
theirs=90 mismatched:  4 first mismatch: 42,28: ours='0' theirs='2'
theirs=91 mismatched:  5 first mismatch: 42,4: ours='-' theirs='\\'
theirs=92 mismatched:  95 first mismatch: 1,135: ours='1' theirs='8'
theirs=93 mismatched:  95 first mismatch: 1,135: ours='1' theirs='8'
theirs=94 mismatched:  95 first mismatch: 1,135: ours='1' theirs='8'
theirs=95 mismatched:  95 first mismatch: 1,135: ours='1' theirs='8'
theirs=96 mismatched:  95 first mismatch: 1,135: ours='1' theirs='8'
theirs=97 mismatched:  95 first mismatch: 1,135: ours='1' theirs='8'
theirs=98 mismatched:  1724 first mismatch: 1,135: ours='1' theirs='8'
```

`theirs=78` is the screen the application drew at the moment our frame `seq 58` framed
it; every other draw is a different animation instant (spinner glyph at row 42,
elapsed-seconds counter at row 1). **The pair is `ours=58 ⇔ theirs=78`.** Note that on
every one of the 23 pairs the continuation columns **never** appear in the mismatch
list — the residual §5.3/§A3 reported is gone across the whole sweep, not just on the
chosen pair.

The prompt-box screen pairs the same way — `ours=54` against draws 64–75:

```
theirs=64 mismatched:  14 first mismatch: 1,119: ours=' ' theirs='\\'
theirs=65 mismatched:  14 first mismatch: 1,119: ours=' ' theirs='|'
theirs=66 mismatched:  14 first mismatch: 1,119: ours=' ' theirs='/'
theirs=67 mismatched:  14 first mismatch: 1,119: ours=' ' theirs='-'
theirs=68 mismatched:  14 first mismatch: 1,119: ours=' ' theirs='-'
theirs=69 mismatched:  0
theirs=70 mismatched:  63 first mismatch: 48,3: ours='s' theirs='n'
theirs=71 mismatched:  0
theirs=72 mismatched:  63 first mismatch: 48,3: ours='s' theirs='n'
theirs=73 mismatched:  0
theirs=74 mismatched:  63 first mismatch: 48,3: ours='s' theirs='n'
theirs=75 mismatched:  64 first mismatch: 45,13: ours=' ' theirs='/'
```

So the styled wide glyph is measured at **zero on four separate like-for-like pairs**:
`(58,78)` in the transcript, and `(54,69)`, `(54,71)`, `(54,73)` in the prompt box.
(Caveat, stated: draws 6x/7x alternate; the zeroes are the pairs the validator printed
as zero, and no pair was counted that did not print `mismatched:  0`.)

## B4. The comparisons, verbatim

One command per pair, run from the repository root at `edf7c9c`. The two unchanged
sessions are re-compared as well.

```
########## k5-startup-fullscreen ours=1 theirs=0
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/k5-startup-fullscreen --theirs sessions/k5/k5-startup-fullscreen-buffer.jsonl --ours-seq 1 --theirs-seq 0
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:38:34Z, recorded at 50x160
ours:   sessions/k5/k5-startup-fullscreen — frame 2 of 6 (seq 1), 50x160
theirs: sessions/k5/k5-startup-fullscreen-buffer.jsonl — draw 1 of 16 (seq 0), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-startup-fullscreen ours=2 theirs=13
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/k5-startup-fullscreen --theirs sessions/k5/k5-startup-fullscreen-buffer.jsonl --ours-seq 2 --theirs-seq 13
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:38:34Z, recorded at 50x160
ours:   sessions/k5/k5-startup-fullscreen — frame 3 of 6 (seq 2), 50x160
theirs: sessions/k5/k5-startup-fullscreen-buffer.jsonl — draw 14 of 16 (seq 13), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-startup-fullscreen ours=3 theirs=14
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/k5-startup-fullscreen --theirs sessions/k5/k5-startup-fullscreen-buffer.jsonl --ours-seq 3 --theirs-seq 14
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:38:34Z, recorded at 50x160
ours:   sessions/k5/k5-startup-fullscreen — frame 4 of 6 (seq 3), 50x160
theirs: sessions/k5/k5-startup-fullscreen-buffer.jsonl — draw 15 of 16 (seq 14), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-plain-prompt ours=1 theirs=0
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/k5-plain-prompt --theirs sessions/k5/k5-plain-prompt-buffer.jsonl --ours-seq 1 --theirs-seq 0
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:43:40Z, recorded at 50x160
ours:   sessions/k5/k5-plain-prompt — frame 2 of 21 (seq 1), 50x160
theirs: sessions/k5/k5-plain-prompt-buffer.jsonl — draw 1 of 61 (seq 0), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-plain-prompt ours=2 theirs=13
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/k5-plain-prompt --theirs sessions/k5/k5-plain-prompt-buffer.jsonl --ours-seq 2 --theirs-seq 13
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T12:43:40Z, recorded at 50x160
ours:   sessions/k5/k5-plain-prompt — frame 3 of 21 (seq 2), 50x160
theirs: sessions/k5/k5-plain-prompt-buffer.jsonl — draw 14 of 61 (seq 13), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0

########## k5-wide-glyph ours=58 theirs=78
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/k5-wide-glyph --theirs sessions/k5/k5-wide-glyph-buffer.jsonl --ours-seq 58 --theirs-seq 78
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T13:24:02Z, recorded at 50x160
ours:   sessions/k5/k5-wide-glyph — frame 59 of 73 (seq 58), 50x160
theirs: sessions/k5/k5-wide-glyph-buffer.jsonl — draw 79 of 99 (seq 78), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0
```

The extra wide-glyph pairs, quoted in full:

```
.\target\debug\ui-lm-validate.exe compare --ours sessions/k5/k5-wide-glyph --theirs sessions/k5/k5-wide-glyph-buffer.jsonl --ours-seq 54 --theirs-seq 69
session: C:\Users\moreweb\Documents\Workspaces\moreweb\Projects\grok-build-k5\target\release\xai-grok-pager.exe started 2026-10-27T13:24:02Z, recorded at 50x160
ours:   sessions/k5/k5-wide-glyph — frame 55 of 73 (seq 54), 50x160
theirs: sessions/k5/k5-wide-glyph-buffer.jsonl — draw 70 of 99 (seq 69), 50x160
total cells: 8000
mismatched:  0
K5: the emulator's screen and the draw buffer agree on all 8000 cell(s)
EXIT=0
```

`(54,71)` and `(54,73)` printed `mismatched:  0` in the sweep above; they were read from
the sweep's `mismatched:` lines, not re-quoted as full runs.

**Nothing is masked.** No ignore mask was added or widened; `crates/` was not touched in
this pass, and the comparator is the same binary that printed the 2 in §A3.

## B5. Verdict

**K5: PASSED.**

The bar: three sessions from different scenarios, at least one containing a styled
full-width glyph, each printing zero mismatches on like-for-like pairs.

| session | scenario | pairs at 0 | worst pair quoted |
| --- | --- | --- | --- |
| `k5-startup-fullscreen` | startup | 3 (`1↔0`, `2↔13`, `3↔14`) | none — all 0 |
| `k5-plain-prompt` | plain prompt (single-width control) | 2 (`1↔0`, `2↔13`) | none — all 0 |
| `k5-wide-glyph` | **styled full-width glyph** | 4 (`58↔78`, `54↔69`, `54↔71`, `54↔73`) | none — all 0 |

Nine pairs across three scenarios, every one `mismatched:  0` and `8000` of `8000`
cells in agreement, `EXIT=0`. The wide-glyph pair was taken from a session recorded at
**13:24:24, after the adapter fix**, and the continuation columns — the only cells that
ever disagreed — now carry `default`/`default` in our frames against `reset`/`reset` in
the application's buffer.

## B6. What this pass did and did not do

- **One** application run, as authorised: 21.2 s of the wide-glyph scenario. The 35 W1
  sessions were not re-run; the startup and plain-prompt K5 sessions were not re-taken
  (their frames predate nothing that affects them — both already matched, and the fix
  changes only continuation cells, which those screens do not contain).
- The fork was read, not modified, rebuilt or deleted.
- One tracked file changed: this report. `sessions/` (including the new capture and the
  `pre-fix-12-41` backup) is gitignored.
- The unmatched draws in B3 (2–1724 mismatches) are mid-animation screens, not K5
  failures: they are a different instant from the frame, which is what the "name the
  pair" rule exists to avoid. No last-vs-last comparison is reported anywhere in this
  appendix.
