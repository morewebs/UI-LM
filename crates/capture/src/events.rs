//! The application's own event stream, read from its session store on disk.
//!
//! The recorder needs one fact it cannot get from the screen: did the program
//! report anything between these two frames? That fact is what separates a
//! spinner tick from a real change, and without it the volatile-span mask has to
//! assume an event may explain every change, which is why `volatile` was empty in
//! every session recorded before this module existed.
//!
//! Grok Build writes an append-only ACP event stream to
//! `<store>/<bucket>/<session>/updates.jsonl`, one JSON object per line, each with
//! a `timestamp` and a `params.update.sessionUpdate` kind. The store root defaults
//! to `~/.grok/sessions`; the bucket name is the child's working directory with
//! everything outside `[A-Za-z0-9._-]` percent-encoded, so
//! `C:\Users\x\AppData\Local\Temp\a` becomes `C%3A%5CUsers%5Cx%5C%5CAppData...`.
//!
//! The tap reads **lengths, not records**. A longer stream means the program said
//! something; parsing individual events belongs to the labelling work that needs
//! their meaning, and the mask only needs their existence.

use std::fs;
use std::path::{Path, PathBuf};

/// The file a session appends its events to.
const UPDATES: &str = "updates.jsonl";

/// The store root a session's events live under, when a home directory is known.
pub fn default_store() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).join(".grok").join("sessions"))
}

/// The directory name a store uses for a working directory.
///
/// Every byte outside `[A-Za-z0-9._-]` is percent-encoded with uppercase hex,
/// which is what the store does: `C:\a` is stored as `C%3A%5Ca`.
pub fn bucket_name(cwd: &Path) -> String {
    let mut name = String::new();
    for byte in cwd.to_string_lossy().bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_');
        if keep {
            name.push(byte as char);
        } else {
            name.push_str(&format!("%{byte:02X}"));
        }
    }
    name
}

/// Follows one recording's event stream by the only signal a writer gives
/// cheaply: how long the file has become.
///
/// The session directory belongs to the application and is created when it
/// starts, which is after the recorder has already spawned it, so resolution is
/// lazy and may fail for the first few polls. An unattached tap reports `None`
/// rather than zero, and the caller keeps treating events as unknowable until it
/// attaches.
pub struct EventTap {
    store: PathBuf,
    cwd: PathBuf,
    updates: Option<PathBuf>,
}

impl EventTap {
    /// A tap that will look for `cwd`'s event stream under `store`.
    pub fn new(store: PathBuf, cwd: PathBuf) -> Self {
        Self {
            store,
            cwd,
            updates: None,
        }
    }

    /// The byte length of this recording's event stream, or `None` while no
    /// stream exists yet.
    ///
    /// A length rather than a parsed count: it is monotonic, it cannot be
    /// confused by a partially written final line, and appending a line is what
    /// the application does when it reports something.
    pub fn length(&mut self) -> Option<u64> {
        if self.updates.is_none() {
            self.updates = self.locate();
        }
        let path = self.updates.as_ref()?;
        match fs::metadata(path) {
            Ok(found) => Some(found.len()),
            // A stream that disappeared means the session was rotated away under
            // us; stop pretending to follow it.
            Err(_) => {
                self.updates = None;
                None
            }
        }
    }

    /// Whether this tap found a stream at all.
    pub fn attached(&self) -> bool {
        self.updates.is_some()
    }

    /// The session directory's update file, once known.
    fn locate(&self) -> Option<PathBuf> {
        let bucket = self.store.join(bucket_name(&self.cwd));
        let sessions = fs::read_dir(&bucket).ok()?;
        // Session directories are UUIDs whose leading digits are a timestamp, so
        // the greatest name is the newest session. Picking by name rather than by
        // modification time keeps the choice off the filesystem's clock.
        let newest = sessions
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .max()?;
        let updates = newest.join(UPDATES);
        updates.is_file().then_some(updates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_working_directory_becomes_the_bucket_the_store_uses() {
        // Both names are copied from a real store on a machine that ran the
        // application, so this is the spelling the application chose, not one
        // this test invented.
        assert_eq!(
            bucket_name(Path::new(
                r"C:\Users\moreweb\AppData\Local\Temp\ui-lm-k5\k5-plain-prompt"
            )),
            "C%3A%5CUsers%5Cmoreweb%5CAppData%5CLocal%5CTemp%5Cui-lm-k5%5Ck5-plain-prompt"
        );
        assert_eq!(bucket_name(Path::new("/tmp/a-b_c.d")), "%2Ftmp%2Fa-b_c.d");
    }

    /// A store laid out the way the application lays one out, for `cwd`.
    fn store_with(store: &Path, cwd: &Path, session: &str, events: &str) -> PathBuf {
        let dir = store.join(bucket_name(cwd)).join(session);
        fs::create_dir_all(&dir).unwrap();
        let updates = dir.join(UPDATES);
        fs::write(&updates, events).unwrap();
        updates
    }

    #[test]
    fn the_tap_reports_the_newest_sessions_length() {
        let root = tempfile::tempdir().unwrap();
        let cwd = Path::new("/work/project");
        store_with(root.path(), cwd, "01a-old", "{}");
        let newest = store_with(root.path(), cwd, "01b-new", "{}\n{}\n");

        let mut tap = EventTap::new(root.path().to_path_buf(), cwd.to_path_buf());
        assert_eq!(tap.length(), Some(fs::metadata(&newest).unwrap().len()));
        assert!(tap.attached());
    }

    #[test]
    fn a_stream_that_grows_is_visible_to_the_next_poll() {
        let root = tempfile::tempdir().unwrap();
        let cwd = Path::new("/work/project");
        let updates = store_with(root.path(), cwd, "01a", "");
        let mut tap = EventTap::new(root.path().to_path_buf(), cwd.to_path_buf());
        let before = tap.length().unwrap();

        let mut file = fs::OpenOptions::new().append(true).open(&updates).unwrap();
        file.write_all(b"{\"sessionUpdate\":\"agent_message\"}\n")
            .unwrap();
        file.flush().unwrap();

        let after = tap.length().unwrap();
        assert!(after > before, "an appended event must lengthen the stream");
    }

    #[test]
    fn a_missing_stream_is_unattached_rather_than_empty() {
        // The distinction the mask depends on: `None` means "nobody can say",
        // which the caller must not read as "no events happened".
        let root = tempfile::tempdir().unwrap();
        let mut tap = EventTap::new(
            root.path().to_path_buf(),
            Path::new("/nowhere").to_path_buf(),
        );
        assert_eq!(tap.length(), None);
        assert!(!tap.attached());
    }
}
