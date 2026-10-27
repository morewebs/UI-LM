# A capture command changed the user's configuration

Date: 2026-10-27. Recorded because it happened, was caught, and will happen again.

## What happened

Looking for a screen that draws a table, a capture script sent `/model` to the application
and then Enter. `/model` with no argument does not list models: it **switched the session's
model** (the screen said `Switched to grok-4.20-0309-non-reasoning`) and the application
rewrote `~/.grok/config.toml` on the way out, with `[models] default` changed from
`grok-4.7` to `grok-4.20-0309-non-reasoning`.

It was caught by checking the file afterwards, and restored to `grok-4.7` — which brought
the file back to its previously recorded size (878 bytes), confirming the original value.

## Why it matters to this rig

The recorder runs the real application, and the real application honours commands that
write configuration. A capture is therefore not a read-only observation: any keystroke
script that reaches a settings command changes the machine it is recorded on.

The W1 capture already snapshotted and restored `trusted_folders.toml` for exactly this
reason, and the same discipline has to cover `config.toml` and anything else the
application writes.

## What to do about it

- **Prefer commands with no side effect.** `/model` here is a switch, not a listing;
  `/usage` opens an overlay and changes nothing. The difference was not documented and is
  now known.
- **Snapshot and restore the whole store**, not one file, around a scripted capture.
- **Check the mtime afterwards.** The change was visible as a config write at the timestamp
  of the capture — cheaper to notice than to reason about.
- A capture that means to observe a settings screen should still restore afterwards; the
  evidence is the frames, not the state.

## What it does not change

The page model and the frame format are unaffected: a capture observes the screen, and a
configuration change is a side effect of the program, not of the observation. But a corpus
recorded from a machine whose settings drifted between sessions is a corpus with a hidden
variable, which is the research problem worth avoiding.
