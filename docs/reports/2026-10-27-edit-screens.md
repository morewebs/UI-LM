# What the application draws for an edit, and why no diff was ever captured

Date: 2026-10-27. Answers an open question in the W1 capture report, which recorded
that a diff view had never been captured and could not say why.

## The question

The `Diff` component exists because ADR 0002 calls a diff the one thing no surveyed
standard can express. The W1 capture of 35 sessions produced no diff screen. Was that
a rig limitation, a scenario that never asked for an edit, or something about the
application?

## What was run

Two recordings of a file edit in a scratch project:

1. With the application's auto-approval on (the default the earlier capture used):
   *"change the first line of `notes.txt` so it says hello world"*.
2. With `--permission-mode default`, on the theory that asking before an edit would
   raise the approval card that carries a diff.

Both edited the file for real: `notes.txt` ended up holding the new line, so the
scenario reached the tool, not just the model.

## What the application drew

It never drew a diff body. It drew a one-line report of the tool call:

```
♦ Read notes.txt
♦ Edited notes.txt, explored 1 file +1 -1
The second line of notes.txt is now BETA. The file reads:
```

So an edit appears as:

- a tool line marked with `♦`, naming the tool and its target;
- for an edit, an inline stat `+A -R` giving added and removed line counts;
- followed by ordinary prose describing the result.

`--permission-mode default` did not raise an approval card in a directory the
application already trusts, which is why the second run produced the same shape as
the first.

## What this means for the IR

- **`Diff` cannot be proven from this corpus**, and the reason is not the rig: the
  screen an edit produces has no `+`/`-` prefixed lines for the detector to find. The
  detector stays as it is, tested against the shape it looks for and documented as
  unproven on real screens.
- **`ToolCall` is the component this corpus actually needs**, and its evidence is
  concrete: a `♦`-prefixed line, a tool name, a target, and for edits a pair of
  counts. That is enough for `status`, `input` and a change summary, and it is worth
  building next.
- A screen with a real diff body needs a scenario that makes the application render
  one — a mode or setting that shows changes before applying them — which no flag
  the CLI documents did in this directory.

## Method note

The edit lines were read out of recorded pages produced by
`ui-lm-capture page --session <dir> --all`, so this is evidence from the same path
the model will be trained on, not from a side channel.
