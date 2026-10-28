# The floor, and what measuring it revealed about the metric

Date: 2026-10-27. A converter that does nothing was built to be beaten. Measuring it showed
the score is not yet good enough to beat.

## The floor

`ui-lm-capture baseline` splits a screen into paragraphs, calls each one a block of text,
and scores that page against the one the recorder produces.

| screen | structure | text |
|---|---|---|
| the checked-in fixture | 40.0% | 100.0% |
| a directory listing | 100.0% | 100.0% |
| a shell listing | 100.0% | 100.0% |
| the help overlay | 77.8% | 34.8% |
| the context panel | 80.0% | 44.8% |
| the session panel | 80.0% | 45.9% |
| the usage overlay | 80.0% | 52.9% |
| **all 27 frames** | **81.5%** | **49.3%** |

## What the numbers say

Two things, and the second is the important one.

**Text agreement is not a useful signal on real screens.** The naive converter reproduces
every row of the fixture but only a third of the help overlay: a page drawn at fixed rows
does not reflow the way paragraphs do. Text agreement measures layout luck as much as
content.

**The structure score is dominated by the most common component.** A do-nothing converter
scores **81.5%** overall, because most components in a real page *are* blocks, and a
converter that makes everything a block recovers most of them by accident. A model scoring
"84% structure" would sound like progress and mean almost nothing.

So the floor did its job before any model existed: it showed the metric rewards emitting
blocks rather than understanding a screen.

## What the metric needs

- **Weight by rarity.** A `Meter` or a `ToolCall` is harder to get right than a `Block`, and
  a page that misses them should lose more than a page that misses a paragraph.
- **Exclude what is trivial.** If a component kind appears in every reference page, matching
  it says nothing.
- **Report per kind, not only in aggregate.** "Block 95%, Meter 0%" is honest; "81.5%" is
  not.
- **Score structure against a floor-adjusted baseline**, so a number means "better than
  doing nothing" rather than "better than nothing at all".

None of that is done. Until it is, no claim about a converter beating the floor should be
taken seriously — including one this rig might make.