# ui-lm

A page intermediary: one structured description of what is on a terminal screen and what can be done there, produced from the rendered cells **by a trained model**, rendered back as a GUI, and patched as the screen changes. The model decides what each row *is*; the code decides how a component is *shaped*. This file fixes the words. The design and the decisions behind it live in [the spec](docs/superpowers/specs/2026-10-27-ui-lm-design.md).

## Language

### The page and what renders it

**Page**:
One rendered state of an interface at one moment — everything the user currently sees.
_Avoid_: screen, view, state

**Frame**:
A page captured as data: a fixed grid of cells at a known time.
_Avoid_: screenshot, snapshot

**Cell**:
One character position in a frame, with its character and its visual attributes.
_Avoid_: glyph, char block

**TUI**:
An interface a program renders into a terminal as frames.
_Avoid_: terminal app, console app

**GUI**:
An interface rendered by a widget toolkit.
_Avoid_: graphical UI, desktop app

**Widget**:
A toolkit object with its own identity and behaviour. A TUI made of cells has none.
_Avoid_: control, element

**Widget tree**:
The retained structure of an interface's widgets. Its absence is why this project reads cells.
_Avoid_: component tree, element tree

### The representation

**IR** (intermediate representation):
The structured description of a page that sits between the rendered interface and the runtime.
_Avoid_: model output, page spec, AST

**Envelope**:
The standard-defined outer form of the IR: how a surface is declared, a catalogue chosen, components instanced, and actions expressed.
_Avoid_: wrapper, container

**Catalogue**:
A named, versioned set of component definitions an envelope may reference.
_Avoid_: library, theme, kit

**Component**:
One definition in a catalogue; an instance is one node in a surface.
_Avoid_: element, widget

**Surface**:
The envelope's holder for one page's component instances.
_Avoid_: root, document

**Provenance**:
Whether an IR node was read off the frame (`seen`) or inferred from it (`guessed`).
_Avoid_: confidence, certainty

**Patch**:
Model output that edits the existing IR instead of replacing it.
_Avoid_: diff, delta, update

**Action**:
An IR reference to something the runtime knows how to do. In this version, only a keystroke.
_Avoid_: handler, callback, event

**Keymap**:
The versioned vocabulary of keystrokes an action may name.
_Avoid_: bindings, shortcuts

### Building the data

**Rig**:
The recording subsystem, and only that: a process driver, an emulator, and a frame store. It produces the material the model
learns from; it is not the project and not the model.
_Avoid_: harness, scraper, collector; also avoid calling the project "the rig"

**Emulator**:
The part of the rig that turns the bytes a program emits into a frame.
_Avoid_: parser, VT, terminal

**Volatile span**:
A part of a frame that changes without changing meaning, such as a spinner or a timer.
_Avoid_: noise, jitter, animation

**Label**:
The ground-truth IR for one captured frame.
_Avoid_: annotation, ground truth

**Teacher**:
The large model that produces labels offline.
_Avoid_: oracle, annotator

**Student**:
The small model that ships and is meant to match the teacher.
_Avoid_: target model, small model

**Golden set**:
The hand-checked frames the student is measured against.
_Avoid_: test set, benchmark set

### Measuring it

**Verbatim accuracy**:
The fraction of strings in the IR that exactly match the characters on the frame.
_Avoid_: string accuracy, copy score

**Idempotence** (of a patch sequence):
Applying a run of real patches yields the same IR as converting the final frame once.
_Avoid_: convergence, stability

**Round-trip**:
Converting a page to IR and back to an interface of the same modality yields the same actions and the same reachable pages.
_Avoid_: equivalence, fidelity

**Agent CLI**:
A terminal application whose interface is a conversation with an agent and the tool calls it makes.
_Avoid_: coding agent, chat CLI

## The seven kinds

The catalogue holds seven kinds. They are the project's whole vocabulary for what a screen is made of, and each
one carries a narrowing learned from a real screen — the narrowings are the definitions, not footnotes to them.

**Block**:
A run of rows that reads as one thing, carrying a role, its lines, and per-span styling.
_Avoid_: paragraph, text area

**ToolCall**:
A run of marked rows — `♦` from Grok, `•` from Freebuff — each naming a tool, a target and a status.
_Avoid_: action, command

**Thought**:
A reasoning heading and nothing more: the marked word alone, or followed by how long it took. The words under an
expanded heading are a `Block` of their own, and a bullet that keeps talking is a sentence, not reasoning.
_Avoid_: reasoning block, chain of thought

**Meter**:
One drawn quantity against one drawn limit, with the fraction computed beside them. A percentage without a
limit is not a meter, and neither is a status row carrying four figures.
_Avoid_: progress bar, gauge, usage line

**Diff**:
A block holding both an addition and a removal, with a per-line kind. A staged rename is one, and so is a patch
with `+++` and `---` headers.
_Avoid_: patch, change

**Frame**:
A floating window: a box drawn *over* the page rather than beside it, named by the words on its top border. Two shapes so far: heavy rules (vim) and a centred titled box (whiptail); a full-width sheet docked at the edge is not one.
Its rows are the screen's own, so it draws back unchanged.
_Avoid_: modal, dialog box, overlay
