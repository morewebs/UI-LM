# Adopt A2UI's envelope and author terminal vocabulary as a custom catalogue

**Status:** accepted (2026-10-27)

The instruction was "use the most already-adopted standard, don't invent one". No
standard can express a diff primitive, per-span text attributes, fold regions,
progress meters or a fixed-width grid — the closest thing in the entire survey is a
per-`Inline` colour on Adaptive Cards. So terminal vocabulary gets authored whichever
standard is chosen. We took A2UI as the envelope: it is the only standard built for
a model→renderer handoff, it has first-party renderers in four stacks, and
catalogues are its specified extension point, so a custom catalogue is participating
in the standard rather than forking it. The six terminal components live in
`moreweb.terminal/v1`.

## Considered options

- **Adaptive Cards** — the largest installed base in existence (Teams, Outlook, Power
  Platform), but card-shaped, single-vendor, and no way to express a diff at all.
- **Own IR from scratch** — simpler scope and no standard claim, but it forfeits
  negotiation, degradation semantics and renderers that we would otherwise not write.

## Consequences

- A renderer we did not write will render our envelope but **not** our catalogue.
  That must be stated plainly wherever adoption is claimed; calling the catalogue
  "the standard" would be false.
- A2UI v0.9.1 is pre-1.0. The v1.0 candidate may force envelope churn we do not
  control, so the catalogue version is pinned independently of the envelope version.
- If upstream ever accepts the terminal catalogue, the vocabulary becomes portable.
  That is the reason to propose it rather than only publish it.

Design: §6 D8, §4 C11.
