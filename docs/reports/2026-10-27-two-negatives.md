# Two negatives: no second event source, and which glyphs a fixture can carry

Date: 2026-10-27. Both were established while trying to widen the corpus, and both bound
what can honestly be claimed from it.

## The second application keeps no event stream this rig can tap

The standing gap is masking on an application other than Grok Build. Antigravity CLI
(`agy`) has no equivalent of `~/.grok/sessions/<bucket>/<session>/updates.jsonl`:

- `~/AppData/Local/agy` holds only the executable and its previous copy.
- `~/AppData/Roaming/Antigravity` and `~/AppData/Local/Antigravity` are an Electron
  application's directories: blob storage, code cache, GPU caches, Local Storage.
- The only logs are `language_server.log` and `main.log`, written on 2026-10-26 and not
  per session, in a Go/glog text format with no event kinds.

So a second application's masking cannot be shown today. The claim stays exactly what the
evidence supports: **volatile masking works for the one application whose event stream the
rig can read**, and for any other program the mask is empty by design rather than wrong.

The tap is not the limitation — the stream's existence is. Wiring a second application
means finding or building a store it appends to, and that is application-specific work,
not a general improvement to the tap.

## A fixture can carry ASCII and very little else

Extending the checked-in fixture to cover `panel`, `tabs` and `composer` failed, and the
reason is worth recording because it will bite the next person:

- The fixture program writes bytes to the pty, and the host decodes them as an OEM code
  page. `0x04` arrives correctly as `♦`, so CP437 decoding does happen.
- But the CP437 box-drawing bytes do not survive: `0xDA`, `0xC0`, `0xB3` and `0xC4` arrive
  as `U+FFFD`, `U+FFFD`, `U+0133` and `U+013F` respectively — not the box characters, and
  not consistently anything.

So a recorded fixture can hold ASCII, and the tool glyph, and nothing else that matters.
Panels cannot appear in it, which means `panel`, `tabs` and `composer` stay covered by unit
tests over constructed frames rather than by captured bytes. That is a weaker form of
evidence, and it is the strongest the platform allows without a different pty.

## What changed anyway

The composer detector now accepts square corners (`┌`/`└`) as well as rounded ones
(`╭`/`╰`), because an application that draws `╭` on one terminal draws `┌` on another.
That much of the fixture attempt was a real improvement, and it has its own test.
