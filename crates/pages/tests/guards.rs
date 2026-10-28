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

/// The kinds a page carries, in order, ignoring the root column.
fn kinds(page: &Envelope) -> Vec<String> {
    page.create_surface
        .components
        .iter()
        .filter(|component| component.component != "Column")
        .map(|component| component.component.clone())
        .collect()
}

/// Builds a page where every row carries the same label.
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
fn a_tree_branch_starts_its_own_block() {
    // `cargo tree` draws a dependency per row, and the reader gives each branch its own
    // block. A closing glyph that only rules a box (`╰──── Grok ────╯`) still ends the block
    // above it, which is why the rule is written for the branch shape alone.
    let page = page_of(
        &[
            "│   [dependencies]",
            "├── ui-lm-model v0.1.0",
            "├── ui-lm-pages v0.1.0",
            "└── vt100 v0.15.2",
        ],
        "Block",
    );
    // The branch rows that continue a group (`├──`) belong to it; only the last one
    // (`└──`) starts a block of its own — which is what the reader does, measured on
    // `cargo tree`: splitting both ways scored 43.8%, splitting only the last scored 100%.
    assert_eq!(kinds(&page).len(), 2, "{:?}", kinds(&page));
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
