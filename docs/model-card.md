# Model card: `models/page.json`

**What it is.** A softmax regression that labels rows of a terminal screen as one of five kinds — `Block`,
`ToolCall`, `Meter`, `Thought`, `Diff` — plus a rule layer that turns contiguous runs of one kind into
components. It is not a language model and it does not generate anything.

| | |
|---|---|
| weights | `models/page.json`, 19 x 5 matrix and 5 biases, SHA-256 begins `433465EADD618E45` |
| features | 19, read off the screen itself: tool glyphs, meter ratios, indentation, box characters, elapsed-time phrases, diff markers, and so on |
| parameters | 100 |
| inference | Rust, `crates/model`, no runtime dependencies |
| training code | `scripts/train_model.py`, `python3 scripts/train_model.py` |

## Training data

**434 screens from one application**: Grok Build, running under tmux on this machine across a day of
recording. Every screen was paired with the page the *rule-based reader* produced for it — the reader is
the teacher, so the model learns to imitate a program, not to discover structure.

There is no second application in the training data. This is the model's central limitation and it is
stated here rather than in a footnote.

## Evaluation

| set | what it is | result |
|---|---|---|
| curated corpus | 60 recordings, 343 frames, both builders | 100% agreement, no audit violations |
| seven evaluation sets | Grok and non-Grok screens, 299 frames | 100% every set |
| **a second agent CLI** | **15 recordings, 709 frames, never trained on** | **100%, every component** |
| fresh batches | recorded **after** training, 8 batches | 100% every batch, every kind it held |

**The floor matters as much as the score.** `score` also reports a text-only converter that splits
paragraphs; a set where the floor is already 100% cannot distinguish the model from that converter, and
`score` now says so in its output:

```
model 100.0%  floor 100.0%  over 1 frame(s), trained on 434 screen(s)
  the floor reproduces this set exactly, so a perfect score here proves nothing
```

Five of the eight non-Grok recordings are in that state. The three with real floors (50%, 33%, 33%) are
all perfect.

## What it cannot do

- **It cannot grow by finding another kind.** The five kinds are the vocabulary; a sixth would be a change
  to the page format, the reader, both renderers and the checks — see `docs/adding-a-component.md`.
- **It is at its ceiling on this data.** Measured three ways: 60% more corpus gave +0.1% row accuracy; a
  twentieth feature was worse on every screen family; training on plain screens was three times worse.
- **It does not generalise on its own — the assembly does.** Measured with `disagree`, which compares the model's
  row labels with the reader's kinds:

  | application | rows where the model disagrees |
  |---|---|
  | Grok (trained) | **2.0%** to **10.3%** |
  | Freebuff (never trained on) | **17.6%** to **26.3%** |

And the disagreements are not spread evenly — they have one shape, measured by confusion pair:

  | reader says | model says | where |
  |---|---|---|
  | Block | **Thought** | 177 rows on one session, 155 on another |
  | Block | **Meter** | 56 rows, 381 on a long one |
  | Thought / ToolCall | Block | 22–90 rows |

  The model over-reads reasoning and meters on an application whose screens it has never seen, and the guards
  that stop it are the **narrow** ones — a reasoning heading must end after its word, a meter needs a drawn limit
  — both written for other screens, and both doing their job here.

  The pages are still exact on both — because the guards correct what the model gets wrong, not because the model
  learned a second application. **What generalises is the assembly, and the vocabulary it asks the reader for.**
- **Its raw labels disagree with the reader on 2–12% of rows.** Structural guards are what keep pages
  exact; the model is one voice in an assembly, not an oracle.
- **`Meter` has one observed producer.** Agents print a budget — use against a limit — because that is what
  they manage. Progress bars, monitors and file copies print percentages and counts instead, and `as_meter`
  refuses to invent a limit from a percentage. The kind is real; the applications that draw it are few.
- **`Frame` has one producer, and has been confirmed on two machines.** A titled box with corners that floats inside
  the screen: `whiptail` draws one in three modes, and the same dialog recorded on a GitHub runner reads as
  `Frame=1` there too — the check installs `whiptail` to make that question askable. The declines are as tested as
  the accepts: `vim`'s completion popup (a float without a border) and Freebuff's composer (a border touching the
  edge) are both `Block`. **One producer, four examples, three applications, two machines** — a kind the corpus
  supports, thinly, and says so.

- **A budget glued into one token is *declined*, and the cost is measured.** `0/5 hours remaining`, drawn inside the
  second agent's quota panel, is not read as a meter. Accepting it was measured on
  `eval/freebuff/quota-panel` (round 130): the round trip fell from **86/86 to 67/86** because a meter's extent is
  decided by the row that starts it and the reader has no rule that ends one inside a box — it swallowed the panel's
  remaining rows and its closing border. **Nineteen lines for one meter.** The rule stays narrow, and this is now a
  number rather than a preference.

## Every boundary, and what stops it widening silently

Each of these is a place where a rule *could* be widened to catch one more screen, at the cost of reclassifying
screens already in the corpus. Widening is the change that looks like progress, so each boundary names what would
catch it:

| boundary | the rule | the counterexample | the guard |
|---|---|---|---|
| `Frame` | a **titled box with corners**, not touching the screen's edge | vim's completion popup (borderless) and Freebuff's composer (a box touching the edge), both read as `Block` | `check-hard` forbids `Frame` on `vim-completion-popup` |
| a **divider** | a column of bars **not matched** by a bar on the same rows | a centred `whiptail` dialog, whose border looked like a divider | the `tmux-two-panes` and `grok-help-frame6` fixtures, plus `whiptail-menu` |
| `Meter` | a count against a drawn limit; a percentage is not one | `top`'s `%Cpu(s)` header | the `second machine` job forbids `Meter` on a monitor |
| a **listing** | one `Block`, never a meter, thought or tool call | a directory listing, on any machine | the same job forbids all four on a listing |
| `Diff` | **any** addition beside **any** removal | `- buy milk` / `+ buy eggs`, which is not a patch | a test that *pins the cost* rather than forbidding it: 45 of 72 corpus diffs carry no `@@` hunk header (24 carry no header at all), so narrowing was reverted |
| `Table` | does not exist | column alignment matched 28 of 81 recordings, all survey sheets | ADR 0007, rejected on measurement |

**Five boundaries guard against widening; one pins an accepted cost.** The difference matters: forbidding is right
where the wide rule would be *wrong*, and pinning is right where the wide rule is merely *expensive*.

## What it can do that it was not trained for

**A second agent CLI, read without ever seeing it.** Freebuff (`freebuff`, an npm package that draws its own
TUI) was recorded fifteen times, 709 frames, across the whole shape of agent work: reading a file, creating one,
showing a `git diff`, showing a *rename* in a diff, searching several files, a full fix cycle, two refusals where
the task could not succeed, a repository it had never seen, and one session cut short by a quota. Every component
in every frame is reproduced.

| | |
|---|---|
| Block | 4652/4652 |
| ToolCall | 158/158 |
| Thought | 385/385 |
| Diff | 54/54 (two shapes: `+++`/`---` and a `similarity index` rename) |
| Meter | none — freebuff shows `15.4K (1%)` with no limit on screen, so it stays a Block |

That is the claim this model exists to test, and `check-agents` holds it: the recordings are scored, drawn
back, audited, counted, and bounded by shape. Its material is local, like the hard cases.

## Intended use

Describing a terminal screen as structured content, so a renderer that never saw the screen can rebuild
it. `crates/render` does that for HTML, `ui-lm-serve` for a live view, and `ui-lm-capture tui` for text.

## Out of scope

- Interactive programs whose state is not a screen (a daemon, a file).
- Screens whose meaning depends on what was on the screen *before* the frame — the model sees one screen.
- Any decision where being wrong is expensive. It labels rows; it does not verify them.

## Reproducing the numbers

```
powershell -File scripts/check-all.ps1          # the curated axes, both builders
powershell -File scripts/check-fresh.ps1        # record new material and score it
powershell -File scripts/check-diff-fresh.ps1   # the fifth kind, on new material
```