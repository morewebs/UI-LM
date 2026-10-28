//! Exporting only the screens that carry structure.
//!
//! Measured on a real corpus, 20 of 26 settled screens held only prose. A pair whose target
//! is one block of text teaches nothing that copying already gives, so the export can be
//! asked for the minority — the screens a converter has to recognise to get right.

use std::path::PathBuf;
use std::process::Command;

/// The checked-in recording, which holds a meter, a tool call and a diff.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pages/tests/fixtures/session")
}

/// The pairs a `dataset` run writes, as parsed JSON.
fn export(extra: &[&str]) -> Vec<serde_json::Value> {
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("pairs.jsonl");
    let mut command = Command::new(env!("CARGO_BIN_EXE_ui-lm-capture"));
    command
        .arg("dataset")
        .arg("--session")
        .arg(fixture())
        .arg("--out")
        .arg(&path)
        .args(extra);
    let status = command.status().unwrap();
    assert!(status.success(), "the export failed: {status}");
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn the_structured_export_keeps_the_screen_with_a_meter_in_it() {
    let pairs = export(&["--structured"]);
    assert_eq!(pairs.len(), 1, "the fixture's one screen carries structure");
    let target = pairs[0]["target"].as_str().unwrap_or_default();
    assert!(
        target.contains("\"Meter\""),
        "the meter must survive: {target}"
    );
    assert!(
        target.contains("\"ToolCall\""),
        "the tool call must survive"
    );
}

#[test]
fn asking_for_structure_still_gives_the_ordinary_export_when_asked_for_it() {
    // The plain export keeps everything, so the filter is a choice and not a default.
    let plain = export(&[]);
    let structured = export(&["--structured"]);
    assert!(
        plain.len() >= structured.len(),
        "filtering can only remove pairs: {} vs {}",
        plain.len(),
        structured.len()
    );
}
#[test]
fn the_same_screen_twice_is_one_pair() {
    // Recording settles often, so the same quiet screen appears more than once. Exporting
    // it twice makes a corpus look bigger and teaches a model to repeat the commonest
    // screen in it.
    let out = tempfile::tempdir().unwrap();
    let path = out.path().join("pairs.jsonl");
    let status = Command::new(env!("CARGO_BIN_EXE_ui-lm-capture"))
        .arg("dataset")
        .arg("--session")
        .arg(fixture())
        .arg("--session")
        .arg(fixture())
        .arg("--out")
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success(), "the export failed: {status}");
    let body = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "one screen, however many sessions name it");
}
