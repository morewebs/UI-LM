# A corpus of real screens is not publishable as it stands

Date: 2026-10-27. Found while assembling the first multi-program corpus, before anything
was committed.

## What was measured

Four recordings — a help overlay, an information panel, a directory listing, and a small
program drawing a shortcut list — exported to **12 training pairs**, covering 31 blocks,
12 columns, 5 meters and 5 tool calls.

Then the pairs were searched for anything personal:

| check | result |
|---|---|
| mentions the local user's name | **12 of 12** |
| carries a session identifier | 1 of 12 |

Every pair contains at least one absolute path with the account name in it — a working
directory, a session file, a scratch project. A published corpus would publish the machine
it was recorded on.

## Why this matters now, not later

The whole point of the export is to become training data, and training data tends to get
copied, shared and committed. A corpus is much easier to keep clean at capture time than to
scrub afterwards, and a scrub that misses one row is worse than no scrub, because nobody
checks again.

## What a published corpus needs

- **A replacement pass at export**, not at capture: the frames are evidence and should stay
  as recorded; the *pair* is what gets published, so the substitution belongs in
  `dataset`. Paths become `<path>`, identifiers become `<id>`, and the substitution is
  recorded alongside the pair rather than silently applied.
- **A test that no pair contains the account name**, run against the corpus the export
  produces — the same way the audit checks soundness. A rule nobody measures is a rule
  nobody keeps.
- **A decision about whether the substitution is itself a leak**: a screen that shows a
  directory tree still shows its shape, which can be identifying even without the name.

## What was not done

The corpus was not committed and not kept: it lives only in a scratch directory that is
deleted after the run. The export itself is committed, since it is the machinery, not the
data.