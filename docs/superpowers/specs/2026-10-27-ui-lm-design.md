# UI-LM — design

**Status:** revision 2, draft for founder approval.
**Date:** 2026-10-27.
**Provenance:** one grilling session, 2026-10-27, plus a builder review pass. Research
inputs: [prior art](../../../../Notes/2026-10-27-ui-lm-research.md),
[Grok Build's TUI](../../../../Notes/2026-10-27-ui-lm-grok-build-cli.md),
[foundation models](../../../../Notes/2026-10-27-ui-lm-foundation-models.md),
[Liquid AI](../../../../Notes/2026-10-27-ui-lm-liquid-ai.md),
[IR standard](../../../../Notes/2026-10-27-ui-lm-ir-standard.md).

---

## 1. Intent

Agents cannot see interfaces. They get text dumps, screenshots, or app-specific
protocols — each lossy, or available for three applications. UI-LM builds the missing
layer: a **canonical, model-readable description of what is on a page and what can be
done there**, produced from what the interface actually renders, and rendered back
into a real interface by a runtime.

The demo that proves the representation generalises is **terminal UI → GUI**. The
prize is the representation. A converter that works and nothing else is a parlour
trick.

**This is research first.** The studio wants to know whether cell→IR can be made
reliable, and only then wants it lightweight and fast. So the plan has two stages,
and they are evaluated against different questions:

- **Stage 1 — research (6 weeks).** Establish whether the thing works, and publish
  honest numbers: fidelity, drift, generalisation, latency. Optimise for truth, not
  speed. "Something that works" is the milestone.
- **Stage 2 — product.** Make it lightweight, fast, local and shippable: small model,
  low bits, CPU path, packaging, the 20-app suite, the open spec and runtime.

Latency is **measured from week 1** (the harness makes it free) but only **gates in
stage 2**. That way stage 2 starts with the numbers already in hand instead of
discovering them.

**The first application is Grok Build** (xAI's terminal coding agent, `grok`): open
source, already installed on this machine (`1.0.70`, native Windows binary), and its
event stream gives ground truth for free.

### 1.1 The research questions

Everything below serves these. Each has a number attached by §7.5.

| # | Question |
|---|---|
| **RQ1** | Can a cell grid be turned into a structurally correct IR, and at what verbatim fidelity? (The ceiling is the teacher's own accuracy, measured first.) |
| **RQ2** | Do patches stay idempotent over a real session, or does drift accumulate? |
| **RQ3** | How small can the student be before verbatim fidelity breaks? |
| **RQ4** | Does it generalise to other terminal applications? |
| **RQ5** | Is the resulting IR sufficient that a human can do a real task in the GUI without the terminal? |

---

## 2. What it is, concretely

Four artefacts, one repository (`Projects/ui-lm`):

1. **`ui-lm-capture`** — drives a TUI in a PTY, reconstructs the cell grid with a
   VT100 emulator, stores frames deterministically, and aligns frames to the
   application's own event stream to produce labelled data.
2. **`moreweb.terminal/v1`** — an [A2UI](https://github.com/a2ui-project/a2ui) v0.9.1
   **catalogue**: six JSON Schemas for the terminal vocabulary, plus a versioned
   keymap. A2UI's envelope, our vocabulary.
3. **`ui-lm-runtime`** — the official renderer: A2UI surface + our catalogue → GUI in
   moreweb house styling. Owns the canonical IR document, applies model patches, and
   synthesises keystrokes back into the PTY.
4. **`ui-lm-model`** — a small fine-tuned model mapping a cell grid (+ the previous
   IR window) to an IR patch. Never emits behaviour; only structure and references to
   actions the runtime implements.

---

## 3. Goals, non-goals, anti-goals

### Stage 1 goals (research, weeks 1–6)

- **G1 (messy goal).** One person completes a real coding task in the GUI'd Grok
  Build, end to end, and never opens the terminal.
- **G2.** Fidelity numbers on a golden set: verbatim accuracy, drift, and latency,
  reported as numbers rather than adjectives.
- **G3.** A written answer to RQ1–RQ3 and RQ5, including whatever the answer is when
  it is "no".

### Stage 2 goals (product, whoever picks it up next)

- **G4.** Lightweight and fast on ordinary hardware: student model, low bits, p95
  refresh under budget, no GPU required.
- **G5.** Generalisation: a suite of 20 full-screen TUIs with published numbers.
- **G6.** Open spec, open runtime, open weights — anyone can implement the catalogue.

### Non-goals

- **GUI → TUI.** Deferred to stage 2 at the earliest. The reverse direction is
  measured only as idempotence (M5), never shipped.
- **Screenshot / pixel input.** Input is the cell grid as text. A vision path is a
  later component, not a v1 hedge.
- **Aesthetics beyond house tokens.** Styled once, from OpenRemote's tokens.
- **Replacing Grok Build's TUI**, patching its behaviour, or wrapping its features.
- **Cloud inference in the product path.** The teacher is a dev-time tool only (D10,
  D20).
- **The 20-app suite in stage 1.** It is a second bet; ask before starting it.

### Anti-goals

Things that would make this a failure even if it technically works:

- **A bigger model as the answer.** Size is the fourth lever, after (1) always
  grammar-constrain, (2) keep the schema shallow and split calls, (3) fine-tune on our
  screens. Reaching for a bigger model means the schema is wrong.
- **A GUI that mirrors the terminal.** If the result is a styled terminal, we built a
  screenshot tool.
- **A GUI that invents silently.** Whatever the model inferred must be marked.
- **A bespoke Grok Build integration.** If it only works because we read Grok Build's
  source, it is an integration, not UI-LM.
- **Claiming a standard while only we can render it.**
- **Polishing rendering before the metric exists**, and **training before the eval
  exists.**
- **Optimising for speed before there is something to speed up.** That is what stage 2
  is for, and doing it early is how a research result becomes a demo.

---

## 4. Constraints (established facts, with sources)

Verified 2026-10-27 in the research notes above.

| # | Fact | Consequence |
|---|---|---|
| C1 | Grok Build is Rust on `ratatui 0.29` + `crossterm 0.28` — **immediate-mode**, no retained widget tree ([repo](https://github.com/xai-org/grok-build)) | No widget tree. Input must be cells. |
| C2 | It exposes `-p` headless with `--output-format streaming-json` (typed NDJSON: `text`, `thought`, `tool_call`, `tool_call_update`, `plan`, `usage`, `end`), ACP over stdio, and on-disk `updates.jsonl` / `plan.json` | Free ground-truth labels for training and eval |
| C3 | `src/scrollback/render_tests.rs` is 3,751 lines of `Buffer::empty(viewport)` cell asserts; `portable-pty` is a dev-dep | Our VT emulator can be validated exactly against ratatui's own buffer |
| C4 | JSON validity is saturated (median 97.5%) while leaf-**value** accuracy spans 0.69–0.83 ([arXiv 2604.25359](https://arxiv.org/html/2604.25359v1)) | The metric is verbatim copying, not schema validity |
| C5 | Content fidelity vs size: 78.1 (1B) → 86.1 (3B) → 89.7 (7B) ([SLOT, arXiv 2505.04016](https://arxiv.org/abs/2505.04016)) | The fidelity knee is ~3B. Size is a bake-off, not a guess |
| C6 | Quantisation: flat to Q6, step at Q4, collapse below (8B JSON validity 97.2% fp16 → 86.5% at 4-bit) | Q4_K_M / QAD-Q4_0 floor in stage 2. Never Q3 |
| C7 | llama.cpp GBNF *silently skips* unsupported schema features; XGrammar under-constrains ([JSONSchemaBench](https://arxiv.org/html/2501.10868v3)) | Hand-written grammar over a schema subset + always post-validate |
| C8 | `LFM2.5-2.6B`, `8B-A1B`, `24B-A2B` all "always think" | Use **Base** variants, or `1.2B-Instruct` |
| C9 | 100–150 output tokens fit a 1–2 s refresh on mid-range hardware; 300 does not | Patches, not full conversions — a stage-2 constraint that shapes stage-1 data |
| C10 | LFM Open License v1.0 = Apache 2.0 + commercial use conditioned on < $10M revenue | Acceptable; not OSI-approved. Revisit before selling |
| C11 | No candidate IR has a diff primitive, per-span attributes, fold regions, meters or a fixed-width grid | Terminal vocabulary must be authored either way |
| C12 | **Nothing converts a TUI to real widgets; GUI→TUI is unexplored; no round-trip evaluation exists; no GUI+TUI paired dataset exists** | The eval harness is a novel artefact, not glue |
| C13 | pix2code (2017) is this exact architecture; Design2Code judged GPT-4V output "exchangeable" in 49% of real pages | Pixel→UI has a low measured ceiling. Cells-as-text is the tractable path |
| C14 | Workspace rule: no spending without approval; `growth.md` says non-First-win work waits | This is a deliberate, time-boxed bet |
| **C15** | A **~$1,500 Grok Build credit is sanctioned** for capture and the G1 demo | Capture is unblocked. The rig must log per-session spend so burn is visible |
| **C16** | Target machine: **RTX 5060 Ti 16 GB**, i5-13400F, 64 GB RAM; `grok 1.0.70` already installed | `8B-A1B` (~5.5 GB) and the 9B (~5.8 GB) fit at Q4 — the full bake-off is admissible |
| **C17** | Stage 1 is research: correctness first, latency second | Latency is measured (M6) but does not gate until stage 2 |

---

## 5. Approaches considered

**A. Event/protocol client.** Build the GUI straight off ACP/NDJSON events. Cheap,
semantic, near-perfect — for apps that speak ACP, which is one app. Rejected as the
product; **adopted as the labelling mechanism**. This is the option that would make
UI-LM unnecessary, and the reason the first milestone must produce a number, not a
demo.

**B. Cell-grid conversion with a trained model.** Universal input (every TUI emits
cells), hard problem, generalises. **Chosen.**

**C. Widget-tree introspection.** Would have been the cheapest path. Unavailable:
ratatui is immediate-mode. Recorded because it explains why A is tempting, and
because a future retained-tree toolkit should change this decision.

**D. Build the whole stack from scratch (own IR, pretrain own model).** Rejected: C11
means six components get authored either way, but A2UI's envelope brings renderers,
negotiation and degradation semantics; pretraining is out of budget under C14.

---

## 6. Decisions

Numbered for reference; each was a real fork, and the rejected alternative is named
so a future reader knows what was declined.

| # | Decision | Rejected alternative | Why |
|---|---|---|---|
| D1 | **Agent infrastructure, not a product**; GUI↔TUI is the proof | Ship a converter to end users | The representation is the reusable asset |
| D2 | **Input is the cell grid at inference**; the event stream is labels and eval truth only | Inferring from ACP events | ACP exists for one app; cells are universal (C1, C2). See [ADR 0001](docs/adr/0001-cell-grid-as-model-input.md) |
| D3 | **Runtime owns the canonical IR; the model returns patches** | Full re-conversion per frame; model context as memory | Gives stable identity, survives restart, bounds cost (C9) |
| D4 | **Output is IR→IR only.** Actions are declarative references to a fixed keymap | Model emits handlers/state machines | Generated control flow is unmeasurable and unbounded |
| D5 | **Provenance on every node:** `seen` \| `carried` \| `guessed` | Mirror only; or unmarked inference | A mirror is pointless, silent invention is fatal |
| D6 | **The GUI drives the app by synthesising keystrokes into the PTY** | ACP action calls; read-only mirror | Universal, and symmetric with reading cells |
| D7 | **Capture via PTY + VT100 emulator**; fork Grok Build *only* to validate the emulator, then delete the fork | Fork to dump `Buffer`s for training | Training data must equal inference input (C3 validates the emulator) |
| D8 | **A2UI v0.9.1 envelope + custom `moreweb.terminal/v1` catalogue** | Adaptive Cards; own IR from scratch | A2UI is the most-adopted model→renderer standard and catalogues are its specified extension point (C11). See [ADR 0002](docs/adr/0002-a2ui-envelope-custom-catalogue.md) |
| D9 | **First artefact is the capture rig + eval**; the catalogue is derived from real frames | Catalogue first; renderer first | You cannot specify a schema for screens you have not captured |
| D10 | **Teacher = the studio's own `deepseek-v4.1-flash-1017` endpoint, offline in bulk**; dataset frozen; student replaces it | Teacher in the runtime | Offline labelling is cacheable, near-free, and keeps local-first intact |
| D11 | **Student chosen by bake-off on our own golden set** | Commit to 2.5B or 9B now | C4/C5: no published number measures our task |
| D12 | **All three artefacts open** (spec, runtime, weights) | Source-available runtime | A schema nobody may implement against never gets adopted |
| D13 | **~6 weeks to G1** | 3 months; open-ended | Forces the rig small and the eval central |
| D14 | **Kill criteria pre-registered** (§9) | Decide when the numbers arrive | "It's nearly working" is the story that eats three months |
| **D15** | **The IR is document-scoped; the model only sees and patches the visible window.** Off-screen nodes are retained by the runtime and never re-emitted | Viewport-scoped IR; model sees the whole transcript | A transcript GUI that shows one screenful is the mirror anti-goal. Document scope without unbounded prompts needs the split |
| **D16** | **Two latency budgets, and neither gates stage 1:** first paint p95 ≤ 6 s with a skeleton on screen; patch p95 ≤ 2 s including prefill. Both gate in stage 2 | One budget for everything | First paint is not a patch: a 160×50 screen is 1–3k output tokens (C9) |
| **D17** | **A refresh is a frame after 150 ms of quiescence.** Changed-rows-only input is mandatory, and the training data must be generated in that shape | Every PTY byte is a refresh; full grid per call | Prefill alone can exceed a 2 s budget without it (C9, C17) |
| **D18** | **The golden set needs no hand-authored IR.** A `seen` string is verified mechanically against the cell position it claims; the frame is the ground truth | 300–500 hand-authored IR documents | A solo six-week plan cannot hand-author that, and it would measure student-vs-teacher agreement rather than student-vs-screen |
| **D19** | **M3 is a single-pass number.** No retry inside the measurement; the runtime gets at most one retry, and K1 fires on the single-pass figure | Measure the model + validator + re-ask loop | Otherwise nobody can tell whether the model or the loop reached 99% |
| **D20** | **The teacher may run live in development (weeks 4–6) behind a local cache**, never in the product path | Teacher only offline; or teacher allowed in the runtime | W4 needs a live loop to attempt G1; D10's product rule is untouched |
| **D21** | **Six components, and the prompt editor and four blocking cards are real widgets** | Render cards as text and synthesise keys | G1 means typing prompts and answering permission cards; a text card is the mirror anti-goal again |
| **D22** | **GPU for v1, CPU in stage 2**; M6 measured once, both ways, in stage 1 | CPU-only floor | C16 makes the full bake-off admissible now; CPU is a stage-2 optimisation target |

---

## 7. Architecture

```
        grok (ratatui)                                            human
             │                                                      ▲
     keystrokes│                     ┌────────────────────────────────┘
             ▼                      │
   ┌──────────────────┐  cells +  ┌──┴───────────────┐   IR patch   ┌──────────────┐
   │ PTY + VT100      │─ changed ─▶│  ui-lm-model     │─────────────▶│  ui-lm-      │
   │ emulator (rig,   │   rows    │  (grammar-bound, │              │  runtime      │
   │ or live driver)  │  + prev   │  local)          │              │  (A2UI + our  │
   └────────┬─────────┘    IR     └──────────────────┘              │  catalogue)   │
            │                              ▲                       └──────┬───────┘
            │ frames                       │ labels (offline only)        │
            ▼                              │                              │
   ┌──────────────────┐         ┌──────────┴─────────┐          action ref│
   │ frame store      │────────▶│ teacher labelling  │                    ▼
   │ (JSONL, deduped) │         │ deepseek-v4.1-flash│           keymap (versioned)
   └──────────────────┘         └────────────────────┘
            │
            ▼
   ┌──────────────────┐
   │ eval harness     │  M1–M7 on the golden set
   └──────────────────┘
```

### 7.1 Capture rig

- Drive `grok` in a PTY with a **fixed environment** (`TERM`, `COLUMNS`, `LINES`,
  locale, colour mode), recorded with every session.
- VT100 emulator → cell grid: character + fg/bg + bold/italic/underline/reverse +
  cursor + alt-screen flag.
- **Volatile-span masking is a rig requirement.** Grok Build animates (spinner, turn
  timer, streaming cursor). Frames differing only in a spinner glyph will poison patch
  training; known-volatile spans are masked and recorded, not silently dropped.
- Frame record (JSONL): `{session_id, frame_seq, cols, rows, cells, cursor,
  alt_screen, volatile_spans, acp_event_range}`. Cells run-length encoded.
- **Label alignment:** ACP / `updates.jsonl` events give block semantics (tool name,
  status, plan entries, diffs). Align to visual regions by verbatim text match, then
  geometry. Text present in the cells ⇒ `seen`; reconstructed from context ⇒ `guessed`.
- **Emulator validation:** a temporary patch dumps ratatui's `Buffer` per frame; diff
  against the emulator; require exact match on a sample; delete the fork (K5).
- **Cost logging:** every session records the Grok credit burned (C15).

### 7.2 The IR

A2UI surface (`createSurface`, `catalogId: "moreweb.terminal/v1"`), plus:

- **Six components**, each a JSON Schema: `Block` (generic container with a role),
  `Thought`, `ToolCall` (`pending|running|done|error`, input/output), `Diff` (line kind
  `+|-|context` with per-line spans — the thing no existing standard has), `Table`,
  `Meter` (progress/usage).
- **Per-span text attributes** — required for diffs and inline emphasis, and the
  largest gap in every surveyed standard.
- **`source: "seen" | "carried" | "guessed"`** on every instance (D5, D15).
- **`folded: true`** is a structural fact the buffer proves; a folded block's hidden
  content is not on the page and therefore not in the IR.
- **Stable component IDs** that survive scroll (D15) — M5 is impossible without them.
- **Actions:** `functionCall { call: "key", args: { key, modifiers } }` only, over the
  versioned keymap generated from Grok Build's documented keybindings.
- **Degradation:** an unknown component renders as a labelled placeholder, never
  dropped. The runtime's behaviour on unknown components is part of the spec.

### 7.3 Runtime

- Renders A2UI + our catalogue: reuse A2UI's React renderer for the envelope, author
  the six components, and build the **prompt editor** and the **four blocking cards**
  as real widgets (D21).
- Ships as a standalone app (thin native shell around a local web view) — separate
  from OpenRemote, styled from **OpenRemote's design tokens**, dark.
- `guessed` content gets a subtle, consistent affordance — a documented convention,
  not a warning banner.
- Owns: the canonical IR document, patch application, transcript retention, hit
  testing, focus, keystroke synthesis, and the unknown-component fallback.
- Must never be the source of truth for semantics: if the runtime needs to know
  something the IR doesn't say, the IR is incomplete.

### 7.4 Model and serving

- **Teacher:** `deepseek-v4.1-flash-1017` via the studio's OpenAI-compatible endpoint.
  Offline bulk labelling; live in development weeks 4–6 behind a cache (D20). **The key
  lives in `.est/secrets.toml`** and is never written to the repo, a frame, a log, or a
  note.
- **Students in the bake-off:** `LFM2.5-1.2B-Instruct`, `LFM2.5-2.6B-Base`,
  `LFM2.5-8B-A1B` (Base, per C8), with `Qwen3.5-9B` as the reference ceiling.
- **Constrained decoding:** a hand-written GBNF over a documented schema subset,
  generated from the catalogue schemas but reviewed by hand, plus a post-validator
  that rejects and re-asks once (D19). Never trust the grammar for semantics (C7).
- **Fine-tuning:** QLoRA, Unsloth, the 5060 Ti (C16) or free-tier GPU.
- Stage 2 adds: quantisation floor Q4_K_M / QAD-Q4_0, high-precision KV cache, CPU
  path, packaging (C6, D22).

### 7.5 Eval

Golden set: real Grok Build frames, sampled across the enumerated screen list (§8, W2),
including box-drawing, ANSI colour runs, CJK full-width, tabs, meaningful trailing
spaces, quotes/backslashes, and **near-duplicate consecutive frames** (patch stability).

| # | Metric | Bar |
|---|---|---|
| M1 | Schema-valid rate | Sanity only, expected ~100% |
| M2 | Per-field exact-match F1 | Reported |
| M3 | **Verbatim string exact-match, single pass** (D19) | **The discriminator.** Stage 1 reports it; K1 fires below 95% |
| M4a | **`seen`-node verification**: every string marked `seen` appears verbatim at the claimed cell position | **100%, mechanical** — this is what catches invented-but-marked-seen |
| M4b | `guessed`-node precision | Human spot-check on a sample |
| M5 | Patch idempotence: applying N real patches yields the same **visible window** as converting the final frame once, and the transcript invariant holds for off-screen nodes | The round-trip substitute (D15 reformulation) |
| M6 | Latency p50/p95 for first paint and for patch, **prefill included**, measured once on both GPU and CPU | Measured in stage 1, gates in stage 2 (C17) |
| M7 | Scripted task completion in the GUI | G1's automated half |

The 20-app suite (G5) reuses M1–M6 unchanged. If it needs a different metric, the
metric is wrong.

---

## 8. The six weeks

Each week has something falsifiable.

- **W1 — rig.** PTY driver, VT100 emulator, frame store, volatile-span masking,
  changed-rows diffing. Fork-based emulator validation (K5). Enumerate Grok Build's
  screens from the frame captures. ~30 sessions; spend logged.
- **W2 — catalogue + eval floor.** Derive `moreweb.terminal/v1` from those frames;
  publish the screen enumeration. Rule-based baseline converter scores M1–M6. Teacher
  labelling pipeline running.
- **W3 — scale the data.** Golden set built and sampled across the enumeration,
  adversarial cases included. Teacher dataset frozen. **M3/M4a measured on the
  teacher's own output** — that number is the ceiling the student can ever reach.
- **W4 — runtime.** Six components, prompt editor, blocking cards, house tokens,
  keystrokes to the PTY. Teacher live behind the cache. **Attempt G1.**
- **W5 — bake-off.** Four students × ±grammar, plus M6 on GPU and CPU. Latency measured,
  not gating.
- **W6 — decide.** Pick the student on M3/M4a. Hit G1 and G2 or fire a kill criterion.
  Write up RQ1–RQ3 and RQ5, including negative results. Decide whether G5 is next.

---

## 9. Kill criteria (pre-registered)

Stage-1 kill criteria — fire any one, stop and say so:

- **K1.** Verbatim field accuracy (M3, single pass) below **95%** for *every* student
  candidate after the bake-off, **and** below **90%** for the teacher itself (in which
  case cell→IR is not tractable at this input fidelity and the approach is wrong).
- **K4.** At week 6, the GUI still needs the terminal open to complete a real coding
  task.
- **K5.** The VT100 emulator cannot reproduce ratatui's `Buffer` from PTY output within
  the validation sample. *(Added by the designer: without this the input path is
  unsound and no later number means anything.)*

**Demoted to stage-2 targets** (measured in stage 1, not gating): patch p95 ≤ 2 s,
first-paint p95 ≤ 6 s, ≤150 output tokens per patch, runnable without a GPU.

Explicitly **not** kill criteria: the model being bigger than hoped; the GUI being
ugly; needing a GPU in stage 1.

---

## 10. What's left to the builder

1. **Language and libraries** for the rig and runtime, subject to a mature VT100
   emulator, PTY support on Windows, and reuse of A2UI's React renderer.
2. **Rule-based baseline.** "Transpiler first" cannot hold literally for cell input,
   because cells→IR is not deterministic. Reinterpreted as a **rule-based baseline
   converter** — box-drawing and prompt-line detection, keymap-hint parsing — whose job
   is to *measure where heuristics fail* and become the eval's floor. Its accuracy is a
   data point, not a deliverable.
3. **Patch wire format** (JSON Patch RFC 6902 vs a bespoke op set), given stable IDs.
4. **How `guessed` is rendered** — a convention, not a design project.
5. **Where OpenRemote's design tokens live** and whether they are reusable as-is.
6. **Windows/ConPTY specifics**; capture must be platform-parameterised, v1 need only be
   correct on one platform.
7. **The 20-app list** for G5 — deferred until G1 exists.
8. **Exact licence texts** for runtime and weights (open is decided; the file is
   clerical).
9. **Failure UX**: what the GUI shows on an unparseable patch or an unknown mode.
10. **Quota discipline**: split the ~$1,500 (C15) across W1 capture, W3 capture and the
    G1 demo, and log per-session burn.

---

## 11. Constraints on the builder

- **Spend is sanctioned once, and bounded:** the ~$1,500 Grok credit (C15). Everything
  else — models, GPUs, APIs — stays at zero. Log burn per session.
- **No secrets in the repo.** The teacher key lives in `.est/secrets.toml`. During this
  session a different key was pasted into a chat transcript and should be rotated; the
  rig and the runtime must never log either.
- **Don't patch Grok Build permanently.** The fork exists only to validate the emulator
  (D7, K5) and is deleted afterwards.
- **Measure before optimising.** No renderer polish and no training before §7.5 exists.
  Stage-1 latency work is measurement only (C17).
- **G5 needs a decision, not drift.** 20 apps is a second bet; ask before starting it.

---

## 12. Calls the designer made (not asked)

So they can be challenged while reading:

1. **Provenance marking**, and its third value `carried` — decided after "just figure it
   out". One field is the difference between a mirror and a guess.
2. **Document-scoped IR, window-scoped attention** (D15) — the reviewer's sharpest
   finding. Chosen because G1 implies an unbounded transcript, and because letting the
   model patch nodes it cannot see would make provenance meaningless.
3. **Two latency budgets, both demoted from kill criteria** (D16, §9), because stage 1
   is research.
4. **A mechanically verifiable golden set** (D18) — turns the largest labour item into an
   afternoon of tooling. The frame is the reference; a `seen` string either matches the
   cells or it does not.
5. **M3 as a single-pass number** (D19).
6. **Reinterpreted "transpiler first"** as a rule-based baseline scorer (§10.2).
7. **Windows/ConPTY first**, because that is where the founder works and where `grok` is
   installed.
8. **Four students, not two** (C5 puts the knee at ~3B; `8B-A1B` straddles latency and
   quality).
9. **K5 added** — emulator fidelity as a kill criterion.
10. **Naming**: `Projects/ui-lm`, catalogue `moreweb.terminal/v1`, envelope pinned to
    A2UI v0.9.1.
11. **Volatile-span masking** as a rig requirement rather than a cleaning step.
12. **The screen enumeration is a W2 deliverable**, not a W0 guess — the golden set is
    sampled across it, so the coverage claim is defined before the number exists.
