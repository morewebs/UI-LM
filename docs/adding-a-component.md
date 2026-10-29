# Adding a component kind

A kind is a way of saying what a run of rows *is* — a panel, a tool call, a meter, a diff, a reasoning
line. Adding one touches seven places, and there is a check for each. This page is the list, in the order
that keeps the tree green.

## 1. The body carries it

`crates/pages/src/lib.rs`, `enum Body`. A new variant with the fields the kind needs, `#[serde(rename)]`
matching the catalogue's name, and — if it holds text — a `lines` vector parallel to its rows, because
the round trip is measured against the screen and every line has to be placeable.

The variant must be reachable by `component_text` (for the renderers and the browser check) and by
`audit` (which checks ids, children, and spans against line ends). If `audit` cannot describe the new
shape, add it there first: the audit is the thing that stops a page no renderer could draw.

## 2. The reader produces it

`crates/pages/src/lib.rs`, where components are built from lines. A predicate decides when the run is
this kind; it should live in one function and be called from both builders.

**Both builders.** The reader (`page_from_lines`) and the label path (`page_from_labels`) share every
decision — `panes_hold_structure`, `as_meter`, `as_diff`, `is_thought_row` are the existing ones. A rule
implemented twice is a rule that will disagree with itself; the pane-split gate did exactly that, and
cost six rounds to notice.

## 3. The model can label it

`crates/pages/src/labeled.rs` and `crates/model/src/lib.rs`. The model labels rows; a new kind needs a
label the model can emit, which means retraining and a new evaluation set. **If the kind is rare, do not
train for it** — add the guard that recognises it structurally (as `as_meter` and `as_diff` do) and let
the label path call the same guard.

## 4. The renderers draw it

- `crates/pages/src/render_text` — the screen-faithful one, used by the round trip;
- `crates/render/src/lib.rs` — the browser document, as spans, with a class name;
- `crates/serve` draws through `render_page`, so it follows the browser one.

## 5. The checks prove it

Run, in this order:

```
powershell -File scripts/check-audit.ps1      # the page describes a screen that could exist
powershell -File scripts/check-all.ps1        # both builders agree on it
powershell -File scripts/check-html.ps1       # its words and structure reach the document
powershell -File scripts/check-corpus.ps1     # every recording still reads
powershell -File scripts/check-hard.ps1       # the hard recordings did not get worse
```

Then make each one fail with the new kind deliberately broken — `docs/checks.md` says how each is
proven — and add the kind to whatever the failure taught you.

## 6. The pair export carries it

`dataset --scrub` writes pages into training pairs; a new kind must survive `Scrubber::apply` unchanged
apart from personal data. Run `scripts/check-scrub.ps1`.

## 7. The record says so

`docs/reports/2026-10-28-state-of-the-project.md` claims what is proven, and every row has a measurement
behind it. A new kind without a row in that table is a kind nobody has checked.

## What "done" means

- `check-everything -Quick` green, and the new kind's own proof recorded in `docs/checks.md`;
- a recording that actually contains it, kept where the suite can reach it;
- and, if it changes the page's shape, an ADR saying why — `docs/adr/0004` and `0005` are the two
  examples.