# A blocking card is blocks and shortcut data, not a component

**Status:** accepted (2026-10-27)

The folder-trust card was captured and described by the existing vocabulary without a
change to it: a heading block, an explanation block, and a choice list that comes out as
`shortcuts` (`Yes, proceed = y`, `No, quit = n`). The question was whether that is enough,
or whether a card deserves its own component.

It is enough, for now. The card is not a bordered panel — it is rows of text with a
question and a list — so the structures that describe it already exist. A `Card`
component would have to be filled by the same three blocks to say anything, which is a
name without a shape.

## Considered options

- **A `Card` component** with `title`, `body`, `choices` — expresses the four blocking
  cards the spec names, but the shape was inferred from one card. The overlay that turned
  out to be a tab strip rather than a table is the cautionary case: structure guessed from
  a single screen was wrong once already.
- **Nothing at all** — leave the card as prose. Rejected because the choices are the part a
  runtime must reproduce, and they are real data today.

## Consequences

- A runtime must recognise a blocking card from its parts: a question, a risk explanation
  and two or more `shortcuts`. That is inference at the renderer, and it is the price of
  not versioning the catalogue on one screen.
- The second card type is the trigger to revisit this. If two of the four blocking cards
  share a shape the blocks cannot express — an input field, a diff of pending edits — the
  decision changes, and the evidence for it will exist.
- `shortcuts` stays on `Block` rather than becoming a component for the same reason.

Design: §7.3 (runtime), D21.
