# The checks, and how each was made to fail

A check that has never failed is a check of unknown strength. This is the ledger: every script in
`scripts/`, what it protects, and the failure that was produced to prove it can fail.

**Where a row quotes a message, that is what the check actually printed when it was broken on purpose.**
Five rows do not quote one yet — they were proven when they were written, in the early part of the month,
and the output was not kept. They are marked *(proven, message not kept)* rather than given a quotation
that would be reconstructed from memory; each is being re-proven with its output kept.

| check | protects | how it was made to fail |
|---|---|---|
| `gate.ps1` | clippy, fmt, tests | an unformatted function in a shipped file printed "the gate is closed: 2 check(s) failed, nothing is committed" |
| `check-all` | every curated axis, both builders | **not yet re-proven.** Attempts this month made the reader and the label path change *together*, which this check cannot see — it compares the two builders, so it catches divergence and not a systematic error. Its earlier proof is remembered, not kept. |
| `check-corpus` | every recording reads with both builders | a session with no readable frame produced "no page at all" |
| `check-audit` | pages agree with their screens, both builders | a masked span placed outside its screen produced a violation |
| `check-html` | words and pane structure reach the document | forcing the renderer flat printed "no pane wrapper" |
| `check-hard` | the hard recordings are no worse | disabling the paned renderer printed "draws back at 23.8%" |
| `check-validate` | the validator agrees and disagrees correctly | a validator forced to accept everything printed "the validator accepted a dump with a wrong cell" |
| `check-sizes` | the smallest and largest screens round trip | demanding more lines than exist printed "only 30 line(s) of content, so nothing was really drawn" |
| `check-scrub` | a shared pair keeps its catalogue | removing the protection printed "the catalogue's name was scrubbed away" |
| `check-corrupt` | damaged recordings are skipped, not fatal | rewording the skip message printed "nothing said the frame was skipped" |
| `check-race` (proven, message not kept) | the viewer survives redraws and half-written files | a truncated append made the viewer draw a torn frame |
| `check-interaction` (proven, message not kept) | typing reaches the program | a dropped keystroke left the expected output missing |
| `check-live-page` (proven, message not kept) | the live page moves, a finished one does not | a frozen update made the page stop changing |
| `check-render-scale` | exporting stays linear | a 50 ms delay per page printed "a page cost 53.1 ms, more than the 11.0 ms limit" |
| `check-fresh` | new material matches the model | an older model scored below the floor on the same batch |
| `check-diff-fresh` | a diff on new material is read exactly | removing the diff request printed "no diff appeared, so the check measured nothing" |

And four properties that run under `cargo test`, each proven by mutation:

| property | how it was made to fail |
|---|---|
| 416 screens make no panic and no invalid page | the audit was forced to refuse every page: "416 screen(s) produced a page the audit refuses" |
| 300 pages draw without dropping a word | `render_component` was made to write nothing: "1637 page(s) drew badly" |
| 300 screens through the model make no bad page | the audit was forced to refuse: "300 page(s) from the model fail the audit" |
| 300 byte streams make a consistent frame | the emulator's panic was reproduced: a wide glyph on a 1x1 screen |

**Three checks also refuse to pass on empty material**, which is a separate rule: `check-audit` when it
inspected no recording, `check-sizes` when under eight lines came back, `check-diff-fresh` when no diff
appeared.