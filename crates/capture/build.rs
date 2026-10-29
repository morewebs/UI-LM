//! Stamps the binary with what it was built from.
//!
//! Three times in one week a measurement was taken from a binary that did not carry the change being measured,
//! and each time the result looked like a finding. A stamp makes the question answerable by asking the binary
//! rather than by remembering: `ui-lm-capture version` prints the commit and a digest of the working tree, so a
//! measurement can name the exact source state that produced it.
//!
//! Everything here is best-effort. A shallow clone in CI may have no `git`, and a build must never fail because
//! a provenance detail is unavailable — it reports `unknown` instead.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim().to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn main() {
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());

    // The working tree decides whether this binary carries an uncommitted change, so it is named by a digest of
    // `git status` plus the diff itself. "dirty" would be true before and after an edit — exactly the case the
    // stamp exists for.
    let status = git(&["status", "--porcelain"]).unwrap_or_default();
    let diff = git(&["diff"]).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325_u64;
    for byte in status.bytes().chain(diff.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let tree = if status.is_empty() {
        "clean".to_owned()
    } else {
        format!("{hash:016x}")
    };

    println!("cargo:rustc-env=UI_LM_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=UI_LM_BUILD_TREE={tree}");

    // Rebuild when the sources or the commit change, so the stamp cannot go stale behind an edit or a checkout.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    // A commit does not change .git/HEAD ("ref: refs/heads/main" stays put), but it does append to this log.
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../pages/src");
    println!("cargo:rerun-if-changed=../render/src");
    println!("cargo:rerun-if-changed=../model/src");
}
