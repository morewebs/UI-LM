# The checks, and how each was made to fail

A check that has never failed is a check of unknown strength. This is the ledger: every script in
`scripts/`, what it protects, and the failure that was produced to prove it can fail.

**Every row quotes a message the check actually printed when it was broken on purpose.**
Nineteen scripts and four properties were each made to fail, and the output kept — five of them re-proven in the last week
because their first proof had been remembered rather than recorded.

| check | protects | how it was made to fail |
|---|---|---|
| `gate.ps1` | clippy, fmt, tests | an unformatted function in a shipped file printed "the gate is closed: 2 check(s) failed, nothing is committed" |
| `check-all` | every curated axis, both builders | making the label path report a different role printed "fields: 688 component(s) compared, 2 differing" | Attempts this month made the reader and the label path change *together*, which this check cannot see — it compares the two builders, so it catches divergence and not a systematic error. Its earlier proof is remembered, not kept. |
| `check-corpus` | everything ever recorded reads without an error — and a failed program holding nothing is caught | every recording reads with both builders | a session with no readable frame produced "no page at all" |
| `check-audit` | pages agree with their screens, both builders | a masked span placed outside its screen produced a violation |
| `check-html` | words and pane structure reach the document | forcing the renderer flat printed "no pane wrapper" |
| `check-hard` | the hard recordings are no worse — in score, in round trip, and in shape | disabling the paned renderer printed "draws back at 23.8%"; lowering a component ceiling printed "busiest frame holds 2 components, the ceiling is 1"; and blinding a kind's predicate printed "vim-floating-popup — no Frame component, and this case exists for it", the same for Thought and Meter |
| `check-validate` | the validator agrees and disagrees correctly | a validator forced to accept everything printed "the validator accepted a dump with a wrong cell"; and with the recording made deterministic the check says "agree on all 400 cell(s)" and "disagree on 1 of 400 cell(s)" |
| `check-sizes` | the smallest and largest screens round trip | demanding more lines than exist printed "only 30 line(s) of content, so nothing was really drawn" |
| `check-scrub` | a shared pair keeps its catalogue | removing the protection printed "the catalogue's name was scrubbed away" |
| `check-agents` | the second Agent CLI reads perfectly, untrained — and its material holds structure | with the recordings moved away: "no second-agent recordings on this machine, so nothing was exercised"; with a prose-only recording swapped in: "every component was one kind (Block), so the set tests nothing"; with a lowered ceiling: "busiest frame holds 10 components, the ceiling is 3"; and blinding a predicate printed "no Thought component anywhere in the set, and the set exists for it", the same for Diff |
| `check-corrupt` | damaged recordings are skipped, not fatal | rewording the skip message printed "nothing said the frame was skipped" |
| `check-readme` | the README describes every command the binary has | replacing a command's name in the README printed "the README never mentions: disagree" |
| `check-docs` | the documents' numbers match the tree | the page said v0.10.0 while v0.13.0 was tagged: "the state page names a release that is not the newest one"; and a spelled count printed "spells 'eighteen checks' - counts belong in digits here" |
| `check-race` | the viewer survives redraws and half-written files | flipping the blank-screen test printed "the viewer served the blank screen a program left behind instead of the last one with content" |
| `check-interaction` | typing reaches the program | neutralising **both** key writers printed "interaction failed: the typed text never appeared on screen" |
| `check-live-page` | the live page moves, a finished one does not | a constant document printed "the page did not change across three seconds of a drawing program" |
| `check-render-scale` | exporting stays linear | a 50 ms delay per page printed "a page cost 53.1 ms, more than the 11.0 ms limit" |
| `check-fresh` | new material matches the model, and the batch finishes | an older model scored below the floor on the same batch; with the time limit at one second it printed "recording the batch took 262 s, beyond the 1 s limit" |
| `check-diff-fresh` | a diff on new material is read exactly | removing the diff request printed "no diff appeared, so the check measured nothing" |

And four properties that run under `cargo test`, each proven by mutation:

| property | how it was made to fail |
|---|---|
| 416 screens make no panic and no invalid page | the audit was forced to refuse every page: "416 screen(s) produced a page the audit refuses" |
| 300 pages draw without dropping a word | `render_component` was made to write nothing: "1637 page(s) drew badly" |
| 300 screens through the model make no bad page | the audit was forced to refuse: "300 page(s) from the model fail the audit" |
| 300 byte streams make a consistent frame | the emulator's panic was reproduced: a wide glyph on a 1x1 screen |

## The rule underneath them: nothing is judged from nothing

**Eleven of the nineteen checks refuse to pass on empty material.** The count was written as three for a week,
which is what a rule kept in prose rather than in code is worth — it was true when it was written and nobody
counted again. Each guard prints its own sentence:

- `check-audit` — it inspected no recording
- `check-corpus` — there are no recordings, or none carries an exit status
- `check-sizes` — under eight lines of content came back
- `check-diff-fresh` — no diff appeared
- `check-html` — there are no recordings
- `check-agents` — there is no model, or no second-agent recordings
- `check-hard` — the hard cases are not on this machine
- `check-corrupt` — there is no recording to damage
- `check-scrub` — no recording with a path was found
- `check-docs` — there is no state page, or no corpus to count
- `check-readme` — there is no README, or the handoff no longer lists the commands

The reason is the floor's reason, one level up: **a check that passes on nothing has proved nothing**, and a
green tick from an empty set is worse than a red one, because it looks like evidence.