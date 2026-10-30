//! What a label is allowed to mean.
//!
//! Every one of these cases is a screen that failed in front of me, and each fix was a
//! structural check the reader makes and the label path did not. They are tests rather than
//! notes because the fixes are invisible until a specific screen appears — a table whose
//! status column uses `✓`, a package listing that says `-- (empty)`, an input line with a
//! slash in it — and a future change to labelling would break them silently.
//!
//! The rule they enforce: a page must never claim a component the text cannot justify, no
//! matter what a model says the row is.

use ui_lm_pages::page_from_labels;
use ui_lm_pages::Envelope;

// The kinds a page carries, in order, ignoring the root column.
fn kinds(page: &Envelope) -> Vec<String> {
    page.create_surface
        .components
        .iter()
        .filter(|component| component.component != "Column")
        .map(|component| component.component.clone())
        .collect()
}

// Builds a page where every row carries the same label.
fn page_of(text: &[&str], label: &str) -> Envelope {
    let text: Vec<String> = text.iter().map(|line| (*line).to_owned()).collect();
    let labels: Vec<String> = text.iter().map(|_| label.to_owned()).collect();
    page_from_labels(&text, &labels, "test")
}

#[test]
fn one_added_line_is_not_a_diff() {
    // `+ New Agent` is a button on the application's own screen. A diff is a change, and one
    // line is not a change of anything.
    let page = page_of(
        &["  + New Agent              Open Previous /resume"],
        "Diff",
    );
    assert_eq!(kinds(&page), ["Block"]);
}

#[test]
fn a_run_of_changes_is_a_diff() {
    let page = page_of(&["  - Apples      12", "  + Apples      14"], "Diff");
    assert_eq!(kinds(&page), ["Diff"]);
}

#[test]
fn a_package_listing_is_not_a_diff() {
    // `npm ls` prints `-- (empty)` when there is nothing installed.
    let page = page_of(&["`-- (empty)"], "Diff");
    assert_eq!(kinds(&page), ["Block"]);
}

#[test]
fn a_session_card_is_not_a_thought() {
    // The application's session list, which the reader keeps as one block of two rows.
    let page = page_of(
        &[
            "    ○ HelpBnk discover co-founder            3d",
            "      No co-founder; Yaren builder",
        ],
        "Thought",
    );
    assert_eq!(kinds(&page), ["Block"]);
}

#[test]
fn a_marked_row_is_a_thought() {
    let page = page_of(&["  ♦ Thought for 0.6s"], "Thought");
    assert_eq!(kinds(&page), ["Thought"]);
}

#[test]
fn a_status_table_is_not_a_page_of_thoughts() {
    // `gh run list`: every row opens with a tick or a cross and none of them is reasoning.
    let page = page_of(
        &[
            "✓  test: the application failing        CI   main   push   1m5s",
            "X  docs: three words moved the model    CI   main   push   1m17s",
        ],
        "Thought",
    );
    assert_eq!(kinds(&page), ["Block"]);
}

#[test]
fn an_input_line_is_not_a_meter() {
    // The composer, which holds a slash because the command does.
    let page = page_of(&["> /import-memory            11:02 AM"], "Meter");
    assert_eq!(kinds(&page), ["Block"]);
}

#[test]
fn a_limit_and_a_use_is_a_meter() {
    let page = page_of(&["1.3K / 500K | Storage"], "Meter");
    assert_eq!(kinds(&page), ["Meter"]);
}

#[test]
fn an_echoed_message_is_not_a_tool_call() {
    // The user's own prompt, echoed above the answer.
    let page = page_of(&["Read notes.txt first line"], "ToolCall");
    assert_eq!(kinds(&page), ["Block"]);
}

#[test]
fn a_marked_row_is_a_tool_call() {
    let page = page_of(&["  ♦ Explored 1 search"], "ToolCall");
    assert_eq!(kinds(&page), ["ToolCall"]);
}
#[test]
fn a_real_git_diff_is_a_diff_even_when_every_row_is_called_text() {
    // `git diff` is ten rows, and the first two are `diff --git` and `index`. A model
    // reading them one at a time calls most of them ordinary text, so the run must be
    // offered to the reader's own diff check rather than trusted to the label.
    let page = page_of(
        &[
            "diff --git a/inventory.txt b/inventory.txt",
            "index 4640e9a..c19acd6 100644",
            "--- a/inventory.txt",
            "+++ b/inventory.txt",
            "@@ -1,3 +1,3 @@",
            "-apples 12",
            "-pears 7",
            "+apples 14",
            "+pears 9",
        ],
        "Block",
    );
    assert_eq!(kinds(&page), ["Diff"]);
}
#[test]
fn a_tree_branch_joins_the_listing_it_belongs_to() {
    // Decided in round 26: `cargo tree` is one listing, not one component per dependency. Both paths
    // used to split at every `└──`, which turned a tree screen into seventeen blocks; the model copied
    // the reader, so both agreed and both were wrong. This test pinned the old behaviour and had to be
    // changed on purpose — which is what a pin is for.
    let page = page_of(
        &[
            "│   [dependencies]",
            "├── ui-lm-model v0.1.0",
            "├── ui-lm-pages v0.1.0",
            "└── vt100 v0.15.2",
        ],
        "Block",
    );
    assert_eq!(
        kinds(&page),
        vec!["Block".to_owned()],
        "one listing, one block"
    );
}
#[test]
fn a_second_full_screen_application_splits_without_new_code() {
    // The design deferred a `Frame` component and bet on roles instead, on the strength of
    // one application. This is the screen that tests the bet: `tmux` draws its own panes and
    // status bar, and the same rule finds them with no change.
    let screen = include_str!("fixtures/screens/tmux-two-panes.txt");
    let text: Vec<String> = screen.lines().map(|line| line.to_owned()).collect();
    let labels = vec!["Block".to_owned(); text.len()];
    let page = ui_lm_pages::page_from_labels(&text, &labels, "tmux");
    let panes: Vec<_> = page
        .create_surface
        .components
        .iter()
        .filter(|component| {
            matches!(&component.body, ui_lm_pages::Body::Block { role, .. } if role == "pane")
        })
        .collect();
    assert_eq!(panes.len(), 2, "two panes, found by the same rule as vim");
}
#[test]
fn numbered_tabs_are_tabs_and_a_table_row_is_not() {
    // `1 one.txt  2 two.txt` is how most editors draw tab pages; without this it was
    // described as a line of plain text and a runtime drew words instead of tabs. The rule
    // needs the number on every field, so an ordinary table's first row stays a table row.
    let page = page_of(&["1 one.txt  2 two.txt"], "Block");
    let tabs = page
        .create_surface
        .components
        .iter()
        .filter_map(|component| match &component.body {
            ui_lm_pages::Body::Block { tabs, .. } => Some(tabs.clone()),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(tabs, ["one.txt", "two.txt"]);

    let table = page_of(&["Item        Count    Price"], "Block");
    let none = table
        .create_surface
        .components
        .iter()
        .filter_map(|component| match &component.body {
            ui_lm_pages::Body::Block { tabs, .. } => Some(tabs.clone()),
            _ => None,
        })
        .flatten()
        .count();
    assert_eq!(none, 0, "a table header is not a tab strip");
}
#[test]
fn a_box_border_is_not_a_tree_branch() {
    // `└── name` is a branch of a tree and starts a block; `└─────┘` closes a box and belongs to
    // the block it closes. Both begin with the same two characters, and treating them alike split
    // a multiplexer's popup into two components — 90.5% against a floor of 100%.
    let popup = page_of(
        &[
            "        ┌────────────┐",
            "        │POPUP TEXT  │",
            "        └────────────┘",
        ],
        "Block",
    );
    assert_eq!(
        kinds(&popup).len(),
        1,
        "a box is one component: {:?}",
        kinds(&popup)
    );

    let tree = page_of(&["└── vt100 v0.15.2"], "Block");
    assert_eq!(kinds(&tree).len(), 1);
}
#[test]
fn a_real_bordered_popup_is_one_panel() {
    // Recorded from tmux's `display-popup`, not drawn for this test. The window's four borders
    // and its contents are one component; a tree-branch rule once split the bottom border off
    // and the model fell to 90.5% against a floor of 100%.
    let screen = include_str!("fixtures/screens/tmux-popup.txt");
    let text: Vec<String> = screen.lines().map(|line| line.to_owned()).collect();
    let labels = vec!["Block".to_owned(); text.len()];
    let page = ui_lm_pages::page_from_labels(&text, &labels, "popup");
    let panels = page
        .create_surface
        .components
        .iter()
        .filter(|component| {
            matches!(&component.body, ui_lm_pages::Body::Block { role, .. } if role == "panel")
        })
        .collect::<Vec<_>>();
    let kinds = kinds(&page);
    assert_eq!(panels.len(), 1, "one panel: {kinds:?}");
}

#[test]
fn a_real_popup_over_panes_keeps_its_overlay() {
    // Also recorded: a popup drawn across a two-pane split. The overlay survives as a panel;
    // the panes beneath it are a known limitation, recorded rather than asserted away.
    let screen = include_str!("fixtures/screens/tmux-panes-popup.txt");
    let text: Vec<String> = screen.lines().map(|line| line.to_owned()).collect();
    let labels = vec!["Block".to_owned(); text.len()];
    let page = ui_lm_pages::page_from_labels(&text, &labels, "popup-panes");
    let panels = page
        .create_surface
        .components
        .iter()
        .filter(|component| {
            matches!(&component.body, ui_lm_pages::Body::Block { role, .. } if role == "panel")
        })
        .count();
    assert_eq!(panels, 1, "the overlay is a panel: {:?}", kinds(&page));
}

#[test]
fn word_art_is_not_a_thought() {
    // Freebuff draws its name in full blocks over five rows. The model, which has not seen a full block
    // in its training screens, called that run reasoning. A run with no letter or digit in it cannot be
    // a thought, whatever a model says: it is a picture.
    let page = page_of(
        &[
            "███████████",
            "███▀▀▀▀▀▀▀▀",
            "███ █████  ██████",
            "███ ███▀▀",
        ],
        "Thought",
    );
    assert!(
        !kinds(&page).contains(&"Thought".to_owned()),
        "a sentence must not become reasoning"
    );
}

#[test]
fn a_reasoning_heading_the_model_called_a_tool_becomes_a_thought() {
    // Freebuff draws `• Thinking`; a model trained on Grok calls it a tool call. The reader calls it a
    // Thought, and four separate places in the label path had to be taught to ask the reader:
    // `parse_tool_line` (which glyph), the ToolCall guard (which marker), `settled` (the promotion) and
    // `body_for` (its own copy of the rule, which knew only `thought`, never `thinking`).
    let page = page_of(&["  • Thinking"], "ToolCall");
    assert_eq!(kinds(&page), vec!["Thought".to_owned()]);
}

#[test]
fn a_bullet_that_starts_with_thinking_is_not_reasoning() {
    // A list item may begin with any word, including "Thinking". Grok's collapsed line is `♦ Thought for
    // 3.0s` and Freebuff's heading is `• Thinking` — both end there. A row that keeps talking is a
    // sentence, and calling it reasoning would invent a component the screen does not have.
    let page = page_of(
        &["• Thinking about the problem", "  is what I do at night"],
        "Block",
    );
    assert!(
        !kinds(&page).contains(&"Thought".to_owned()),
        "a sentence must not become reasoning"
    );
}

// A bullet list at the margin reads as one tool call, and that boundary is deliberate.
//
// Every application in the corpus draws a list inside a panel — a changelog, a menu — and the box
// guard keeps those rows together as one `Block`. A bare list at the margin has never appeared, and no
// rule here can tell `• Read the docs` from `• Read notes.txt` without inventing a vocabulary of verbs.
// This test pins the boundary so that a future change cannot move it silently: either way it moves,
// this line stops being true and somebody has to decide why.
#[test]
fn a_bare_bullet_list_reads_as_one_tool_call() {
    let page = page_of(&["• Read the docs", "• Write tests"], "Block");
    assert_eq!(
        kinds(&page),
        vec!["ToolCall".to_owned()],
        "the documented boundary"
    );
}

#[test]
fn a_heading_drawn_inside_a_panel_belongs_to_the_panel() {
    // The mirror of the scrollbar bug: a marker hidden *behind* a leading glyph. `│  ♦ Thinking…` occurs
    // seven times in the corpus — Grok draws its reasoning inside a panel — and both builders keep such a
    // row with the panel rather than lifting it out as a Thought. That is a decision, and this pins it:
    // a panel is one thing, and its interior is described as part of it.
    let page = page_of(
        &[
            "   ╭──────────────────────────────╮",
            "   │  ♦ Thinking…                 │",
            "   │  the words of the thought    │",
            "   ╰──────────────────────────────╯",
        ],
        "Block",
    );
    assert_eq!(
        kinds(&page),
        vec!["Block".to_owned()],
        "a panel is one block"
    );
}

#[test]
fn a_heavy_ruled_box_is_a_frame() {
    // ADR 0006. Written after the round-49 measurement disagreed with the round-48 claim: this decides which
    // of the two was wrong, and it decides it without a recording.
    let rows: Vec<&str> = vec![
        "~                          ╔Confirm═════════════════╗",
        "~                          ║                        ║",
        "~                          ║  Floating dialog       ║",
        "~                          ╚════════════════════════╝",
    ];
    let page = page_of(&rows, "Block");
    let kinds = kinds(&page);
    assert!(
        kinds.contains(&"Frame".to_owned()),
        "a heavy-ruled box must be a Frame, not a block: {:?}",
        kinds
    );
}

#[test]
fn two_prose_lines_read_as_a_diff() {
    // The same boundary as the bare bullet list: at the margin, `- buy milk` beside `+ buy eggs` satisfies the
    // rule for `Diff`, which asks for an addition and a removal. Real lists are drawn inside panels, where the
    // box guard keeps them together, and a patch always carries `---`/`+++` headers — so the cost is a screen
    // no application here draws. Narrowing the rule to require a hunk header was tried in round 55 and reverted;
    // round 61 measured why it must stay wide: 34 of the corpus's 61 `Diff` components carry no header at all,
    // because Freebuff shows an edit as a bare `-`/`+` pair beside the line it changed. Measured again in round 144 over 91 recordings: 61 `Diff` components, **45 with no `@@` hunk header** — the same 34 the round-61 count found, now stated with its definition.
    let page = page_of(&["- buy milk", "+ buy eggs"], "Block");
    assert_eq!(
        kinds(&page),
        vec!["Diff".to_owned()],
        "the documented boundary"
    );
}

#[test]
fn a_new_file_diff_is_a_diff() {
    // It qualifies because `--- /dev/null` supplies the removal row — and that dependence is now known to be
    // harmless rather than load-bearing: a patch may carry no header at all (Freebuff's edit previews do not)
    // and still be a diff, so nothing here rests on git's headers being present.
    let rows = [
        "diff --git a/added.txt b/added.txt",
        "new file mode 100644",
        "--- /dev/null",
        "+++ b/added.txt",
        "@@ -0,0 +1,3 @@",
        "+alpha",
        "+beta",
        "+gamma",
    ];
    let page = page_of(&rows, "Diff");
    assert_eq!(kinds(&page), vec!["Diff".to_owned()]);
}

#[test]
fn a_glued_n_of_m_is_not_a_meter_yet() {
    // `as_meter` looks for a token that *is* a slash, so it accepts `25K / 500K` and declines `2/5`. Grok spaces
    // its slash; Freebuff does not. Measured in round 62: 191 meters in the corpus, every one of them Grok's,
    // and 35 declined rows that look like N/M — among them `2/5 hours remaining`, which is a budget with a
    // limit, and `quick question 1/11`, which is a step counter. Telling those two apart is a decision nobody
    // has made, so this pins today's answer: the glued form stays a block.
    let page = page_of(&["│ 2/5 hours remaining │"], "Block");
    assert_eq!(
        kinds(&page),
        vec!["Block".to_owned()],
        "the documented boundary"
    );
}
