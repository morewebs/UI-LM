//! Helpers shared by the capture crate's integration tests.
//!
//! This module exists because one of these helpers was copied into both test files
//! and a macOS-only fix was applied to one copy, leaving the other red. Two files
//! cannot drift if there is only one copy.

/// A directory in the form two of them can be compared in.
///
/// Windows reports a path in whatever case it likes, with either separator, and
/// `std::fs::canonicalize` would add a `\\?\` prefix the child never prints — so
/// neither a byte comparison nor the canonical form is the right test.
///
/// On macOS a temporary directory lives behind the `/var` to `/private/var` symlink,
/// so the directory a test made and the directory the child reports are the same
/// place spelled two ways. Resolving both compares what the filesystem says rather
/// than how it is spelled.
pub fn comparable(path: &str) -> String {
    let trimmed = path.trim();
    let trimmed = trimmed.strip_prefix(r"\\?\").unwrap_or(trimmed);
    if cfg!(windows) {
        trimmed.replace('/', "\\").to_lowercase()
    } else {
        std::fs::canonicalize(trimmed)
            .map(|resolved| resolved.to_string_lossy().into_owned())
            .unwrap_or_else(|_| trimmed.to_owned())
    }
}
