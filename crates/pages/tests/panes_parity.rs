//! One label per row cannot describe two panes.
//!
//! The label path hands the same array of labels to both sides of a split screen
//! (`labeled_page(&left, labels, ...)`), so the left cell and the right cell of a row receive the
//! same label whatever the model said. The screen below is the smallest one that shows it: row 1 is
//! a meter on the left and a shell command on the right, so a faithful page holds **one** meter.

use ui_lm_pages::page_from_labels;

fn screen() -> Vec<String> {
    (0..10)
        .map(|row| {
            let left = if row == 1 {
                "1.2K / 500K".to_owned()
            } else {
                format!("plain row {row}")
            };
            let right = if row == 1 {
                "ls -la".to_owned()
            } else {
                format!("shell output {row}")
            };
            // Fixed widths, so the divider lands on the same column on every row.
            format!("{left:<20}│{right:<20}")
        })
        .collect()
}

fn labels_for(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .map(|(row, _)| {
            if row == 1 {
                "Meter".to_owned()
            } else {
                "Block".to_owned()
            }
        })
        .collect()
}

#[test]
fn a_meter_on_one_side_does_not_appear_on_the_other() {
    let lines = screen();
    let labels = labels_for(&lines);
    let page = page_from_labels(&lines, &labels, "split");
    let panes = &page.create_surface.panes;
    assert_eq!(
        panes.len(),
        2,
        "the screen is split, so the page must say so"
    );
    let meters = page
        .create_surface
        .components
        .iter()
        .filter(|component| component.component == "Meter")
        .count();
    assert_eq!(meters, 1, "one meter on the left is one meter");
}

#[test]
fn the_synthetic_screen_is_a_split() {
    let lines = screen();
    assert_eq!(lines.len(), 10);
    let dividers = lines
        .iter()
        .filter(|line| line.chars().nth(20) == Some('│'))
        .count();
    assert_eq!(dividers, 10, "every row must carry the divider");
}
