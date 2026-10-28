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
