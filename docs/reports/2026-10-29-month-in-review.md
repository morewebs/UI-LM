# The month in review

**UI-LM, 2026-10-27 to 2026-10-29.** 18 releases, 351 commits, eighteen checks and four fuzz
properties. This page is the short version; the long version is `2026-10-28-model.md`, and the current
state of the product is `2026-10-28-state-of-the-project.md`.

## What it is

A recorder and a describer. A program is run on a pty, its screens are captured as frames, and each
frame is turned into a **page** — a structured description of what the screen holds: panels, tabs,
shortcuts, properties, tool calls, meters, reasoning, diffs, and the panes of a split screen. A learned
model reads the same screens. Two renderers draw pages back: one as terminal text, one as a browser
document.

The promise is not "this turns a TUI into a GUI". It is narrower and testable: **the page is a faithful
description of the screen, and anything that can read the page can rebuild the screen.**

## What the month produced

| | |
|---|---|
| releases | **v0.1.0 → v0.13.0**, three platforms each, every one verified by downloading it |
| checks | **18 scripts**, one table, **every check shown failing with its output kept** |
| fuzz properties | 4 — reader, renderers, model, raw bytes |
| fresh material | **476 frames, seven batches, not one component missed** |
| hardest recording | two agents in two panes: **describes 100%, draws back 100%** |

## The five defects worth remembering

1. **The audit examined nothing.** `audit --session` accepted a flag it ignored, audited zero sessions,
   and printed "no violations". It now takes directories, counts what it looked at, and fails when the
   count is zero.
2. **A page could lose half a screen.** Two panes were read as one flow, so a split screen's right-hand
   side was reported as content of the left. The container fixed it; the gate that decides a split is
   now the same function on both paths.
3. **The model's pages had duplicate ids.** Found within seconds of pointing the audit at model pages —
   a fix that had been applied to the reader two rounds earlier and missed the label path.
4. **Scrubbing renamed the catalogue.** A shared training pair carried `<user>.terminal/v1` instead of
   the real catalogue id, because the identifier contains the organisation's name. Every scrubbed pair
   the project ever made was wrong, and no scoring check could see it.
5. **A paned page was drawn by a renderer that stacked components.** Two full panes overlapped
   themselves: 76% of lines landed in the wrong column while agreement between the two builders was
   perfect. The metric watched agreement; nobody watched the drawing.

## What the month learned about checking

- **Every check now carries its proof.** docs/checks.md lists all seventeen scripts and four fuzz\n  properties with the failure each one actually printed when it was broken on purpose — five of them\n  re-proven in the final week because their first proof had been remembered rather than recorded.\n- **A check that cannot be shown to fail is not a check.** Three of them refuse to pass on empty
  material for exactly this reason: the audit on zero recordings, `check-sizes` on under eight lines,
  `check-diff-fresh` on zero diffs.
- **The defect is usually in what nothing was pointed at**, not in what is hard. The diff, the browser
  check, the corpus check, the model audit and the paned renderer were all one question away: *what
  material has this never been given?*
- **Checks are code and fail the same ways.** One assertion was appended after the script had already
  decided its exit code, matched compact JSON against pretty-printed output, and asserted on a recording
  with no panes — three bugs in the check, found only by running it.
- **A flaky test teaches people to re-run instead of to look.** One cost a re-run on ubuntu before it
  was fixed to look for its path on any line rather than the first.

## What is not proven

- **A second agent's transcripts.** Every screen ever recorded came from one application. The
  vocabulary's generality is assumed, not measured — and this is the one claim the project cannot test
  for itself.
- **Wide-character strings.** A wide glyph fills two cells and the page records both, so a row's text
  reads `日 本 語`. The page is column-exact; the *string* is padded. Widths are measured correctly —
  a marker after two wide glyphs lands in the right column — so this is a format question, not a bug.
- **Layout.** `check-html` compares words and pane structure; it cannot tell whether a page's
  arrangement matches the screen's.

## How to check any of it

```
powershell -File scripts/check-everything.ps1          # 18 checks, ~8 minutes
powershell -File scripts/check-everything.ps1 -Quick   # 18 checks, ~75 seconds
```

Every line in the table can fail, and each was made to fail before it was trusted. The full run includes
five checks that record new material from the training application and score it against the same code
that ships.