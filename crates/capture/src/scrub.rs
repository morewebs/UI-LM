//! Removing the machine from a training pair.
//!
//! A recorded screen is full of the machine it was recorded on: an absolute path carries
//! the account name, a session panel carries an identifier, a prompt carries a home
//! directory. The frames keep all of it — they are evidence, and evidence is not edited.
//! The *pair* is what gets copied, shared and committed, so the pair is where the
//! substitution belongs.
//!
//! # What this removes, and what it does not
//!
//! It removes the account name, the home directory, and anything shaped like a session
//! identifier. It does **not** remove the shape of a directory tree, the names of a
//! person's projects, or a hostname — a scrubbed path still says `C:\…\projects\secret-name`
//! even when the account is gone. That is a judgement for whoever publishes a corpus and
//! not something a substitution pass can settle, so it is named here rather than papered
//! over.
//!
//! The substitutions are visible in the text: `<user>`, `<home>`, `<id>`. A pair that has
//! been through this reads as redacted, which is the honest appearance for it.

use std::sync::OnceLock;

/// What to replace, taken from the machine this is running on.
pub struct Scrubber {
    user: Option<String>,
    home: Option<String>,
}

impl Scrubber {
    /// A scrubber for the machine this is running on.
    ///
    /// The account name comes from the environment the recorder inherited, which is the
    /// same account whose name appears in the screens it recorded.
    pub fn for_this_machine() -> Self {
        Self::with(
            std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .ok(),
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .ok(),
        )
    }

    /// A scrubber for a named account and home directory.
    pub fn with(user: Option<String>, home: Option<String>) -> Self {
        Self {
            user: user.filter(|name| !name.trim().is_empty()),
            home: home.filter(|path| path.trim().len() > 3),
        }
    }

    /// The text with the machine taken out of it.
    pub fn apply(&self, text: &str) -> String {
        // The home directory first: it contains the account name, and replacing the larger
        // string first is what keeps the smaller one from cutting it in half.
        let mut out = text.to_owned();
        if let Some(home) = &self.home {
            out = replace_both_cases(&out, home, "<home>");
            // A screen may spell the same path with forward slashes.
            let slashed = home.replace('\\', "/");
            if slashed != *home {
                out = replace_both_cases(&out, &slashed, "<home>");
            }
        }
        if let Some(user) = &self.user {
            out = replace_both_cases(&out, user, "<user>");
        }
        redact_identifiers(&out)
    }
}

/// `text` with `needle` replaced, whatever case it was written in.
fn replace_both_cases(text: &str, needle: &str, with: &str) -> String {
    if needle.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let lower = text.to_lowercase();
    let target = needle.to_lowercase();
    let mut cursor = 0;
    while let Some(found) = lower[cursor..].find(&target) {
        let start = cursor + found;
        out.push_str(&text[cursor..start]);
        out.push_str(with);
        cursor = start + needle.len();
    }
    out.push_str(&text[cursor..]);
    out
}

/// Anything shaped like a UUID, replaced with `<id>`.
///
/// A session panel shows one, a log line shows one, and an identifier is the sort of thing
/// that follows a person from screen to screen.
fn redact_identifiers(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        if uuid_at(&characters, index).is_some() {
            out.push_str("<id>");
            index += 36;
        } else {
            out.push(characters[index]);
            index += 1;
        }
    }
    out
}

/// The identifier starting at `index`, if one does.
fn uuid_at(characters: &[char], index: usize) -> Option<()> {
    // 8-4-4-4-12 hexadecimal, hyphenated.
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut cursor = index;
    for (position, length) in GROUPS.iter().enumerate() {
        for _ in 0..*length {
            let character = *characters.get(cursor)?;
            if !character.is_ascii_hexdigit() {
                return None;
            }
            cursor += 1;
        }
        if position < GROUPS.len() - 1 {
            if *characters.get(cursor)? != '-' {
                return None;
            }
            cursor += 1;
        }
    }
    Some(())
}

/// Whether the machine's account name appears anywhere in `text`.
///
/// Used as the check a corpus has to pass, rather than as part of scrubbing.
pub fn mentions_account(text: &str) -> bool {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    let name = NAME.get_or_init(|| {
        std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .ok()
            .filter(|name| name.trim().len() > 2)
    });
    match name {
        Some(name) => text.to_lowercase().contains(&name.to_lowercase()),
        // No account name to look for: nothing to claim either way.
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{mentions_account, uuid_at, Scrubber};

    fn scrubber() -> Scrubber {
        Scrubber::with(
            Some("someone".to_owned()),
            Some(r"C:\Users\someone".to_owned()),
        )
    }

    #[test]
    fn the_home_directory_becomes_a_marker() {
        let out = scrubber().apply(r"cwd: C:\Users\someone\projects\thing");
        assert_eq!(out, "cwd: <home>\\projects\\thing");
    }

    #[test]
    fn the_account_name_is_gone_even_on_its_own() {
        let out = scrubber().apply("logged in as someone");
        assert_eq!(out, "logged in as <user>");
    }

    #[test]
    fn a_session_identifier_is_replaced() {
        let out = scrubber().apply("Session ID: 01a17eb7-3a0e-74a3-a87c-0d293f94fa1a");
        assert_eq!(out, "Session ID: <id>");
    }

    #[test]
    fn something_that_is_only_nearly_an_identifier_is_left_alone() {
        let out = scrubber().apply("0123-4567-89ab");
        assert_eq!(out, "0123-4567-89ab");
        assert_eq!(
            uuid_at(
                &"01a17eb7-3a0e-74a3-a87c-0d293f94fa1a"
                    .chars()
                    .collect::<Vec<_>>(),
                0
            ),
            Some(())
        );
    }

    #[test]
    fn ordinary_screen_text_survives_untouched() {
        let text = "Grok Build 1.0.70\n  ♦ Read notes.txt\n1.2K / 500K";
        assert_eq!(scrubber().apply(text), text);
    }

    #[test]
    fn the_check_finds_what_the_substitution_removes() {
        // The same account this test runs under, so the check is exercised for real.
        let name = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_default();
        if name.trim().len() < 3 {
            return;
        }
        let before = format!("path C:\\Users\\{name}\\x");
        assert!(mentions_account(&before), "the check must see the account");
        // The scrubber has to know this machine to remove this machine's own name.
        assert!(
            !mentions_account(&Scrubber::for_this_machine().apply(&before)),
            "and must not see it once scrubbed"
        );
    }
}
