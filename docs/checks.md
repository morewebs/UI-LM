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
| `check-corpus` | everything ever recorded reads without an error — and a failed program holding nothing is caught | every recording reads with both builders | a session with no readable frame produced "no page at all" ; and the pinned diff count in the model card is re-derived from the corpus, proven by quoting 30 instead, which printed "the card says 30 of 61 diffs carry no @@ header; the corpus holds 34 of 61" ; and every kind is carried above its floor, proven by asking for four `Frame` components when the corpus holds three, which printed "a kind is carried by too little material to be claimed" ; and provenance contradictions stay within a pinned ceiling, proven by pinning 20 when the corpus holds 47, which printed "more pages contradict themselves than this project has accounted for" |
| `check-audit` | pages agree with their screens, both builders | a page built by hand with a span past its line, and the assertion made to fail, printed "component \"b\" has a span 0..9 in a line of 2 character(s)" |
| `check-html` | words, pane structure, and that panes are laid out side by side | forcing the renderer flat printed "no pane wrapper"; rendering `data-layout="column"` printed "a paned page was not drawn as panes" |
| `check-layout` | a paned page is drawn side by side, and a frame with no split carries no pane container | stacking the stylesheet with `display: block` printed "the pane container's stylesheet does not lay two tracks out in a row" ; and it now walks every recording that holds a split (15 of them), not one ; both builders and every paned recording (30 documents) |
| `check-second-machine` | a screen recorded on this machine round trips exactly, and reads as the same shape on any machine | recording `cmd /c exit` printed "the screen holds no lines, so a perfect round trip would prove nothing"; applying its shape rule to `cases/hard/pane-conversation` printed "a plain directory listing was read as Meter, which no machine's listing is" ; and a whiptail dialog is a `Frame` on any machine, proven by expecting a `Diff`, which printed "a dialog box -> was read as Column=1, Frame=1, and this screen is a Diff" |
| `check-hard` | the hard recordings are no worse — in score, in round trip, and in shape | disabling the paned renderer printed "draws back at 23.8%"; lowering a component ceiling printed "busiest frame holds 2 components, the ceiling is 1"; and blinding a kind's predicate printed "vim-floating-popup — no Frame component, and this case exists for it", the same for Thought and Meter |
| `check-validate` | the validator agrees and disagrees correctly | a validator forced to accept everything printed "the validator accepted a dump with a wrong cell"; and with the recording made deterministic the check says "agree on all 400 cell(s)" and "disagree on 1 of 400 cell(s)" |
| `check-sizes` | the smallest and largest screens round trip | demanding more lines than exist printed "only 30 line(s) of content, so nothing was really drawn" |
| `check-scrub` | a shared pair keeps its catalogue | removing the protection printed "the catalogue's name was scrubbed away" |
| `check-agents` | the second Agent CLI reads perfectly, untrained — and its material holds structure | with the recordings moved away: "no second-agent recordings on this machine, so nothing was exercised"; with a prose-only recording swapped in: "every component was one kind (Block), so the set tests nothing"; with a lowered ceiling: "busiest frame holds 10 components, the ceiling is 3"; and blinding a predicate printed "no Thought component anywhere in the set, and the set exists for it", the same for Diff |
| `check-corrupt` | damaged recordings are skipped, not fatal | rewording the skip message printed "nothing said the frame was skipped" |
| `check-readme` | the README describes every command the binary has | replacing a command's name in the README printed "the README never mentions: disagree" |
| `check-docs` | the documents' numbers match the tree | the page said v0.10.0 while v0.13.0 was tagged: "the state page names a release that is not the newest one"; and a spelled count printed "spells 'eighteen checks' - counts belong in digits here" ; and every forbidden guard is named in the boundary table, proven by renaming the borderless popup, which printed "the boundary table does not name the guard on vim-completion-popup" ; and the training count on the model card matches the weights, proven by quoting 500 instead of 434, which printed "the model card quotes a training set that is not in the model file" ; and the model card's feature and parameter counts match the weights, proven by claiming 250 parameters, which printed "the model card describes a different model from the one in the repository" ; and the handoff names the newest release, proven by naming v0.19.0 instead, which printed "the handoff names an older release than the newest tag" ; and all three describing documents are checked for the corpus counts, proven by the handoff quoting 88 recordings, which printed "HANDOFF.md says 88 recordings, the corpus holds 97" |
| `check-race` | the viewer survives redraws and half-written files | flipping the blank-screen test printed "the viewer served the blank screen a program left behind instead of the last one with content" ; the child redraws every 400 ms so the screen is quiet long enough to be framed, after a loaded suite produced one frame and the guard refused |
| `check-interaction` | typing reaches the program | neutralising **both** key writers printed "interaction failed: the typed text never appeared on screen" |
| `check-live-page` | the live page moves, a finished one does not | a constant document printed "the page did not change across three seconds of a drawing program" ; and the served document holds the recording's own words, proven by demanding a word it does not serve, which printed "the served document is missing 19 word(s), such as tick" |
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