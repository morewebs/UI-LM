# Implementing `moreweb.terminal/v1`

A guide for a runtime this project did not write. Everything here is checked by the scripts named beside it, so the guide cannot
drift from the code without the suite failing.

## What you are given

A page is one JSON object: an A2UI v0.9.1 message whose surface holds components. The vocabulary is in
[the catalogue](./catalogue/moreweb.terminal-v1.md); the format is decided by
[the schema](./catalogue/moreweb.terminal-v1.schema.json), and every page in this project's corpus satisfies it
(`check-schema`).

Draw a page without any recording:

```bash
ui-lm-capture tui --page page.json              # as terminal text
ui-lm-capture render --page page.json --out page.html   # as a document
```

Those two commands are the reference implementation of what follows.

## What your runtime owes the page

**Draw it.** [The catalogue's "What a runtime must draw"](./catalogue/moreweb.terminal-v1.md#what-a-runtime-must-draw) has a row per
part of a page, and the ten document shapes in "A page may arrive from anywhere" say what a document you did not write becomes.
`check-kinds` and `check-html-to-tui` hold this project to it.

**Carry the facts a reader cannot infer.** Seven `data-*` attributes and two hidden views - the full list is in "What a document
must carry back". If you emit HTML, emitting these is what makes your output readable as a screen again (`check-html`).

**Offer the commands, and send them back.** A `Block` with `shortcuts` holds keys; your runtime should offer them as controls and
post the key back unresolved - `ctrl+r` is a fact about the page, and `0x12` is a fact about a terminal. `check-loop` proves every
binding a page offers is accepted by the viewer.

## How to know you are right

| test | what it proves |
|---|---|
| [the conformance kit](./conformance/) | four stored pages, and the screen each must draw |
| [the schema](./catalogue/moreweb.terminal-v1.schema.json) | your page *is* a page before you try to draw it |
| `check-conformance` | a page that stops drawing its screen names the line that changed |

Start by drawing `conformance/meter/page.json` and comparing with its `screen.txt`: three lines of bar and numbers are enough to
find out whether your first pass is in the right direction.

## What this project will not do for you

**Paint.** Colour, spacing, type and interaction are yours. The page says what is there; it does not say how it looks, which is why
the same page is a terminal screen and an HTML document in the same repository.

**Guess.** Where a page has no structure, the honest answer is text: one line per element, and the catalogue says which element owns
which text. A runtime that invents structure is a runtime whose output this project cannot read back.
