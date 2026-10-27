//! Keystrokes arriving from outside the recorder, while it is recording.
//!
//! A script sends keys on a timetable, which is enough to drive a capture but not to
//! drive a session: a person — or something drawing a page — needs to be able to type at
//! the moment they choose. This is that channel, and it is a file on purpose.
//!
//! A file needs no port, no socket and no listener, works identically on every platform
//! this rig builds for, and cannot be reached from another machine. Whoever holds the path
//! appends bytes and the recorder forwards them to the program's terminal:
//!
//! ```text
//! ui-lm-capture record … --control keys.txt -- --some-program
//! printf 'hello\r' >> keys.txt     # the program receives a typed line
//! ```
//!
//! The recorder reads only what is **new** since it last looked, tracked by byte offset, so
//! a long session does not re-send its history. A file that shrinks — truncated or replaced
//! — is treated as new input from the beginning, because that is what a caller rewriting
//! the channel means.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A file of keystrokes to forward to the child as they appear.
pub struct ControlFile {
    path: PathBuf,
    /// How much of the file has already been forwarded.
    seen: u64,
}

impl ControlFile {
    /// A channel at `path`, forwarding from the beginning of the file.
    pub fn new(path: PathBuf) -> Self {
        Self { path, seen: 0 }
    }

    /// The bytes appended since the last call, and none if nothing has been.
    ///
    /// A file that is not there yet is not an error: the channel may be created after the
    /// recording starts, which is the natural order when a page opens and the person types
    /// into it.
    pub fn take(&mut self) -> io::Result<Vec<u8>> {
        let length = match fs::metadata(&self.path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        if length < self.seen {
            // Truncated or replaced: read it again from the start.
            self.seen = 0;
        }
        if length == self.seen {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&self.path)?;
        let start = usize::try_from(self.seen).unwrap_or(0).min(bytes.len());
        let fresh = bytes[start..].to_vec();
        self.seen = length;
        Ok(fresh)
    }
}

/// Whether a path names a file that exists.
pub fn exists(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::ControlFile;
    use std::io::Write;

    #[test]
    fn only_what_is_new_is_forwarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.txt");
        std::fs::write(&path, b"a").unwrap();

        let mut channel = ControlFile::new(path.clone());
        assert_eq!(channel.take().unwrap(), b"a".to_vec());
        assert!(
            channel.take().unwrap().is_empty(),
            "nothing new was appended"
        );

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"bc").unwrap();
        file.flush().unwrap();
        assert_eq!(channel.take().unwrap(), b"bc".to_vec());
    }

    #[test]
    fn a_missing_file_is_quiet_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut channel = ControlFile::new(dir.path().join("not-yet.txt"));
        assert!(channel.take().unwrap().is_empty());
    }

    #[test]
    fn a_truncated_file_is_read_again_from_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.txt");
        std::fs::write(&path, b"first").unwrap();
        let mut channel = ControlFile::new(path.clone());
        assert_eq!(channel.take().unwrap(), b"first".to_vec());

        // Rewritten shorter than before: the caller means "these are the next keys".
        std::fs::write(&path, b"x").unwrap();
        assert_eq!(channel.take().unwrap(), b"x".to_vec());
    }
}
