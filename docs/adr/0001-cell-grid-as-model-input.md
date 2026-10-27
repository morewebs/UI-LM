# The model's input is the cell grid, not the application's event stream

**Status:** accepted (2026-10-27)

Grok Build already emits a perfect semantic stream — typed NDJSON, ACP over stdio,
on-disk session updates — and any GUI built on it would be trivial and exact. We
rejected that as the model's input and chose the rendered cell grid instead, because
the event stream exists for exactly one application, while every TUI emits cells. A
representation that only works where a protocol happens to exist is not the
representation this project exists to build. The event stream is kept, but only
offstage: it supplies training labels and the eval's ground truth.

## Consequences

- TUI→IR becomes a perception problem rather than a lookup, and is limited by what
  the cells can prove. IR nodes must therefore carry provenance (`seen` / `guessed`).
- The capture rig needs a terminal emulator, and its fidelity becomes a kill
  criterion: if it cannot reproduce ratatui's own `Buffer`, no later number means
  anything.
- The general claim over arbitrary TUIs stays reachable — which a protocol-based
  design would have quietly given up.

Design: §6 D2, §9 K5.
