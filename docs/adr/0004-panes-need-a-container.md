# 0004: panes need a container, and now there is evidence

**Date:** 2026-10-28
**Status:** proposed — not implemented
**Supersedes:** the "approaches 2 now, towards 3 later" decision in
[the full-screen design](../superpowers/specs/2026-10-28-full-screen-applications-design.md)

## The question

A full-screen application can divide its window. How should a page say so?

## What the page does today

A screen that is a single block and carries a column of `|` down most of its rows is split into
two `Block`s whose `role` is `"pane"`. That was approach 2 in the design: a role on a block, costing
no schema change.

It works for the case it was built for, and each pane's text keeps its width so the screen draws
back exactly (89/89 lines on vim's split, 92/92 on tmux's).

## What it cannot do

**A pane cannot hold components.** A pane is one block of text. When a split screen also carries
anything else — a popup, a status line with structure, a meter — the pane rule is never consulted,
because it only runs when the whole screen came out as a single block. Measured on a real
popup-over-panes recording: the page holds `Block(text)`, `Block(panel)`, `Block(text)`, and the
split is simply absent.

A previous attempt to widen the rule (refuse only full-width frames, so a dialog would not mask a
split) changed nothing, because the obstacle was not the box guard but the single-block
precondition. It was reverted.

## The three screens

The design asked for three screens before adding vocabulary. They now exist:

| screen | recording | what the page says |
|---|---|---|
| vim, vertical split | `eval/tui/wsl-vim-vsplit` | two `Block`s, role `pane` |
| tmux, two panes | `eval/tui/wsl-tmux-split` | two `Block`s, role `pane` |
| tmux popup over two panes | `eval/tui/wsl-tmux-panes-popup` | three blocks — **the split is missing** |

The third is the one that shows the limit: two applications divide their window the same way, and an
overlay makes the division disappear.

## Proposal

Add one container component to the catalogue, and make a pane a container rather than a block:

```
Frame
  panes: [ Pane { id, focused, children: [Block, Meter, …] } ]
```

or, if a `Frame` is more than is needed, `Column` gains an optional `panes: [id]`, because `Column`
already exists, already holds children by id, and the page's root is one.

**The minimal version worth arguing for** is the second: panes as identified regions in the existing
container, with a pane's children being ordinary components. That needs no new component kind, only a
new field, and it answers the case the role cannot.

## What is deliberately not decided

- Whether a pane reports focus. vim and tmux both draw a focused pane, and neither page says which
  one is active; that is a second, smaller decision waiting for a screen that depends on it.
- Whether a floating window is a pane with a position. The popup screens suggest it might be, and
  one of them is borderless and therefore invisible to the reader today.

## Why write this down instead of building it

The change touches the page schema, the reader, the label path, both renderers and `compare`, and
`self-check` will hold every one of them to the same rule. That is a day's work with a failing test
at each step, not an afternoon's, and doing it fast on a month's evidence of my own error rate would
be the same mistake this project keeps recording.
## Second screen: a pane holds a diff, and the page cannot say so

Recorded `vimdiff` on a real file pair — two panes, each showing a diff of the same two files:

```
  apples 12                     |  apples 14
  pears 7                       |  pears 9
  cherries 41                   |  cherries 41
```

What the page says: **two `Block`s with role `pane`, each 26 lines of text**, model 100% against a
floor of 66.7%, `fields` 0 differing, round trip 28/28.

So the page knows there are two panes and cannot say that **each pane holds a diff** — the one
component the catalogue exists for. A diff inside a pane is, to the page, a paragraph.

This is the case the ADR predicted and could not previously show: panes are structure *around*
content, and today they are structure *instead of* content.

| screen | what the page cannot say |
|---|---|
| popup over two panes | the screen is split at all |
| `vimdiff` | each pane holds a diff |

Two screens, both real, both in `eval/tui`. The design's rule asks for three before new vocabulary;
this ADR's rule should be the same, so it stays **proposed** and the third screen is the remaining
requirement — a pane holding a meter, a tool call, or a tab strip.
## What it would cost, counted rather than guessed

Before deciding, the change was priced by counting its touch points:

| | |
|---|---|
| files mentioning `Body::Column` | 5 — `pages/lib.rs`, `pages/labeled.rs`, `render/lib.rs`, `render/terminal.rs`, `capture/main.rs` |
| occurrences | 19 |
| construction sites that would need a new field | 11 (8 of them in `pages/lib.rs`, several inside tests) |

So it is mechanical work with a wide blast radius: a new field on `Column`, eleven literals, plus
the reader learning to split before it segments, the renderer learning to draw a pane's children
side by side, the label path learning the same split, and `self-check` holding all of them to one
rule.

**Half a day, with a failing test at each step.** That estimate is the honest answer to "why not
this afternoon", and it belongs in the ADR next to the evidence.
## What the attempt found before it was reverted: panes carry no styling

Before building the container, one smaller question was tested on the `vimdiff` screen: do panes carry
the colour the screen drew? They do not — **both panes reported 0 styled lines**, because the pane
path builds its spans empty. A `vimdiff` window shows its changes in red and green, and the page holds
a plain block.

A fix was written: slice each row's spans at the divider, shifting the right pane's offsets to zero.
It worked — 25 of 26 lines styled in each pane — and then broke something more interesting:

```
fields tui: 119 component(s) compared, 12 differing
  Block role "pane"/"pane" ... spans 15/3
```

The **label path does not split panes the same way**. It only consults the pane rule when a screen
produced a single component, so for `vimdiff` it produces no panes at all, and the model's copy cannot
receive spans it has no components for. Grafting by row fails too: both panes claim the whole screen's
rows, so the row-keyed copy lands the left pane's styling on both; matching by text fixes the pairing
but not the absent panes.

**The whole attempt was reverted**, and the finding is the point: pane styling cannot be fixed
without giving the label path the same pane split, which is the container work this ADR describes.
Listed as another entry in its cost rather than a separate defect.
## Attempted, and the estimate held: it does not fit the tail of a round

Started the change and tried a cheaper shape than the ADR first proposed — the grouping on
`Surface` rather than on `Column`, since `Surface` has five construction sites against eleven.

It went the way the estimate said it would: the field rippled through both page builders, then
through `Surface` literals inside test modules, one at a time, each found by the compiler. At the
point where the round's remaining budget ran out, the tree was mid-change and **nothing useful was
committed**.

Everything was reverted. `cargo test` green, `fields` 0 differing, tree clean, HEAD unchanged.

**Two things are now established by trying rather than by arguing:** the change is mechanical and
does not fight the design — the compiler walked me through every site without a single conceptual
problem — and it is genuinely too large for the last third of a round. The next attempt should start
at the top of a round, or be split into "add the field everywhere, no behaviour" followed by
"use it" in a second.

That second shape is better than what I tried today, and it is what the next attempt will do.
## Step two, measured: the label path is the blocker, and the axes prove it

Re-applied the reader half — panes declared on the surface, each pane's styling sliced at the
divider — and it worked: `vimdiff` panes went from **0 styled lines to 25 of 26 each**.

Then the `fields` axis, which does not care how the change *looks*:

```
fields tui: 119 component(s) compared, 12 differing
```

Before the change it was **0 differing**. Five sessions got worse, and the per-session split says
exactly why: `wsl-vim-vsplit` 1, `wsl-vim-split` 3, `wsl-tmux-split` 3, `wsl-tmux-panes-popup` 4.
In those sessions the model page has **no panes at all**, because the label path still consults the
pane rule only when a screen produced a single component — so the reader now has richer panes and
the model has none.

Reverted. `fields` back to 0 differing, tests green, HEAD unchanged at the field-only commit.

**The lesson is about order, and it took two rounds to run:** adding information to one side of a
mapping that both sides must agree on makes every axis worse until the other side catches up. The
next attempt does *both* sides in one change — the label path's pane condition is the first edit,
not the last — or the reader half stays uncommitted until it is.
## The third screen: an agent running inside one pane of a split

Obtained by running the agent itself in the left pane of a tmux split, through WSL interop — a
detached-session trick failed twice, but driving the agent in a single pane and splitting with
`C-b %` worked:

```
                        │moreweb@HOST:/path$
  24K / 500K │ [Dashboard] │
      > y         7:53 PM  │
      ♦ Thought for 0.4s   │
```

The left pane holds **a meter, a reasoning line, tool calls and a composer** — everything this
catalogue exists for — and the page is a flat list of components across both sides:

| measurement | result |
|---|---|
| frames with a `ToolCall` | 25 of 38 |
| frames showing the divider | 36 of 38 |
| frames declaring panes | **0 of 38** |

**This is the case the ADR was written for, and it is now observed rather than argued.** A split
screen with structured content in one pane produces a page that cannot say which components belong
to which side; a consumer drawing it would put the agent's tool calls and the shell's prompt in one
column.

The three-screen requirement is met: panes without overlays (vim, tmux), panes under an overlay
(popup), and **panes holding components** (this one). The next step is the change the ADR already
priced, and the rule learned in rounds 85–90 applies: both sides of the mapping in one commit.
## Step one, landed: the page says which components share the screen

The `panes` field was dead schema when it was added in round 87 — the styling fix ended up copying
panes by id, so nothing ever filled it — and it was removed in round 91 for that reason. It is back,
and this time **both builders fill it for every split they already detect**: the reader from the
cells, the label path from the text, in one commit, which is the order three failed attempts taught.

| check | result |
|---|---|
| evaluation sets | 100% |
| `fields`, TUI | 119 components, 0 differing |
| agent in a single pane | 100% |
| agent in a split pane | 96.0% |
| `vim` vertical split, panes in the JSON | **2 groups, both pages** |

What this does **not** do is the container itself: a screen whose panes hold components still yields
no grouping, because the split is only applied when the screen reduced to a single block. The target
tests in `crates/pages/tests/panes_as_containers.rs` still fail, by design, and they run with
`--ignored` to say so:

```
left: 0, right: 2 — two panes to look through
```

The next step is therefore exact: apply the split **before** segmentation, per side. The field, the
ids and the styling all already work.
## The split happens before segmentation — written, and it never fired

The exact next step from the last section was implemented in one round: the reader refactored to work
on lines and spans rather than a frame, a recursive per-side read behind an `allow_split` flag, and the
same split in the label path, both in the same commit.

It built, the gate opened, and **the new branch never ran once**: `panes` stayed empty on the
agent-in-a-split screen, score unchanged at 96.0%.

The reason is one layer down, and it is now the recorded starting point:

- `as_panes` refused that screen outright, because **any box corner** on it (`╭ ╮ ╰ ╯`) returned
  `None`. That rule exists so a framed panel is not split at its own right border.
- Replacing the rule with "the divider must have content beside it" did **not** fix it: the divider
  search still prefers the column with the most `│` characters, and on that screen the agent's own
  box border out-scores the real split.
- Moving the same test into the candidate loop did not fix it either.

Reverted. `clippy`, `fmt` and `tests` green, HEAD unchanged.

**What the round established, and it is worth more than the code:** the blocker is not the container
design, it is `as_panes` choosing the wrong column on a screen that is both framed and split. The next
attempt starts there — one function, one screen, and a printed column table — rather than at the page
level where three rounds have now been spent.
## The table answered it in one line

Printing the divider hits for the agent-in-a-split screen was the whole diagnosis:

| column | rows with a divider | rows with content beside |
|---|---|---|
| **55** | **29 of 30** | 3 |
| 52, 40, 36, 18, 2 | 1 each | 30 |

The split was never ambiguous — the real divider held 29 of 30 rows. The only thing refusing it was
the rule that **any box corner** on a screen disables panes, written for a framed panel whose border
could win the vote. On this screen the border wins nothing: one row each.

So the corner rule is gone, and the panel case is left to the hit threshold that already exists:

| check | result |
|---|---|
| evaluation sets | 100% |
| `fields`, TUI | 119 components, 0 differing |
| known-hard screens | 100% and 96.0%, unchanged |

The "content beside the divider" idea from the previous attempt was **also wrong**, and the table says
why: the right pane of that screen is mostly blank, so only 3 of 30 rows have anything beside the
divider. A rule I invented to replace the corner rule would have refused the very screen that
motivated it.

The container branch itself (read each side separately) is still to be applied on top of this; the
`as_panes` blocker in front of it is now removed with a measurement.
## The second attempt at the branch, and the asymmetry it exposed

With the corner rule gone, the per-side read was applied again — the reader refactored to lines and
spans, a recursive read per pane behind `allow_split`. It fired: the agent-in-a-split screen went from
**0 to 2 declared panes**, and the score moved 96.0% → 90.8%.

Then the eval sets, which is why they exist:

| screen | before | after |
|---|---|---|
| `wsl-vim-vsplit` | 100% | **61.1%** |
| `wsl-vim-split` | 100% | 70.0% |
| `wsl-tmux-split` | 100% | 47.9% |
| `wsl-tmux-panes-popup` | 100% | 45.8% |

**The reader now splits screens the label path does not.** The label path only splits when it built
more than one component, and a vim pane of plain text builds one — so the two paths diverge on exactly
the screens that used to be perfect. That is the same asymmetry as rounds 85–90, one layer up: this
time it is not styling or ids, it is *where* the split happens in the pipeline.

Reverted. Everything green at `c86bbad`.

**The next attempt must restructure both paths in the same shape** — a shared "read these lines" entry
that both call, so "split first" is a property of one function rather than a rule each path agrees to
follow. Doing it to one and then the other is how four attempts have now failed.
## The container works — and it is one styling fix away from being committable

Both paths were changed at the same point this time, with the same `as_panes` test: reader and label
path each split **before** segmentation. What that produced, measured:

| check | before | after |
|---|---|---|
| target tests (`panes_as_containers`) | 2 ignored, 1 failed | **2 passed, 0 failed** |
| agent in a split pane | 96.0% | **97.7%** |
| agent in a single pane | 100% | 100% |
| evaluation sets | 100% | **100%** |
| `fields`, TUI | 0 differing | **2 differing** |

**The container itself works**: a split screen declares two panes, every component belongs to exactly
one, and an agent's meter and tool calls no longer sit in the same list as the shell beside them.

The two that differ are styling, not structure: `spans 35/32` on `wsl-vim` and `wsl-vim-vsplit`. The
graft copies pane styling by id, and a pane that now holds its own components has ids derived from
*content* that the two paths can compute slightly differently. Teaching the graft to copy per pane
group instead of per id is the remaining work — small, and the only thing between this branch and a
green tree.

Reverted for now, because `fields` is one of the five claims and a branch that breaks it is not
finished. **The next round starts from this note rather than from a blank page**: the split is proven,
the tests pass, and only the graft is left.
## The seventh attempt: the container works, and three other layers still expect a block called `pane`

Done properly this time — both paths split at the same point, and the graft copies styling **pane by
pane, component by component inside each pane**, plus a fix so the row-keyed pass stops overwriting
pane rows. Results before the sweep:

| check | before | after |
|---|---|---|
| target tests, un-ignored | 2 failed | **3 passed** |
| `fields`, TUI | 0 differing | **0 differing** (132 compared, up from 119) |
| agent in a split pane | 96.0% | **97.7%** |
| every evaluation set | 100% | 100% |

Then the two checks that `fields` and `score` cannot see:

```
round trip:       6 session(s) below 100%   (wsl-vim, wsl-vim-split, wsl-vim-vsplit, …)
round trip/model: 6 session(s) below 100%
tests: 1 failing
```

**The container is correct at the page level and wrong everywhere else**, because two more layers key
on the old shape: the terminal renderer draws panes by looking for blocks with role `pane`, and a unit
test asserts the same role. A pane that holds components no longer has that role, so nothing draws it
side by side any more.

Reverted. Green at `0dea6e6`: `fields` 688 with 0 differing, round trip 0 sessions below 100%, gate open.

**The remaining work is now a list rather than a mystery**, and it is short: the renderer must draw
panes from the surface's grouping, the role-based test must read the grouping instead, and the
label path's pane blocks must stop being a special case. That is one change across three files, with
the page level already proven.
## One of the three call sites is done: the renderer draws from the grouping

The renderer used to find panes by looking for two blocks with role `pane`. It now reads the page's
own grouping, keeping the role as a fallback so an older page still draws:

```text
groups = page.create_surface.panes   (falling back to role "pane")
left  = rows of every component in groups[0], in order
right = rows of every component in groups[1], in order
```

That is green on its own — `fields` 688 with 0 differing, both round trips at 0 sessions below 100%,
every check passing — because for the pages that exist today the grouping and the role say the same
thing. **It is the prerequisite that made the container's round trip fail last round**, and it is now
in place and shipped rather than reverted.

The page-level split was applied again on top, and the remaining gaps are exactly the two predicted:

| check | with the split |
|---|---|
| `fields` | 2 differing (pane styling — the graft fix from round 114) |
| round trip | 4 sessions (tmux/vim splits, both paths) |

Reverted the split, kept the renderer. The list is now **two items**, not three.