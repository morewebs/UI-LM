//! What a keybinding on a page means, as bytes for the program.
//!
//! A page carries shortcuts the way the application drew them — `Ctrl+N`, `/dashboard`,
//! `Shift+Tab` — because that is what a person reads. A program waiting on a terminal wants
//! bytes. This is the translation, and it is deliberately literal: it knows the bindings
//! that appear on the screens this rig has recorded, and it refuses anything else rather
//! than guessing.
//!
//! Refusing matters more than coverage. A wrong guess types garbage into someone's running
//! program, and a button that does nothing is a much better outcome than a button that
//! presses the wrong key.

/// The rest of `text` after `prefix`, whatever case the prefix was written in.
fn strip_prefix_either_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    if text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}
/// The bytes a binding sends, or nothing when the binding is not one this knows.
///
/// - `Ctrl+<letter>` is the control character of that letter, which is what a terminal
///   sends for it: `Ctrl+N` is `0x0E`.
/// - `Shift+Tab` is the one binding with no control character, so it is the sequence the
///   terminal sends: `ESC [ Z`.
/// - A slash command is typed, and then answered with Return, because that is how a person
///   runs it.
/// - A binding naming a destination after an arrow — `Ctrl+P → worktree` — sends the part
///   before the arrow. The part after it is where the program then puts the person, which
///   is the program's business and not a keystroke.
pub fn keystrokes(binding: &str) -> Option<Vec<u8>> {
    let binding = binding.trim();
    if binding.is_empty() {
        return None;
    }
    let first = binding.split('→').next().unwrap_or(binding).trim();

    match first {
        "Tab" => return Some(vec![b'\t']),
        "Enter" | "Return" => return Some(vec![b'\r']),
        "Esc" | "Escape" => return Some(vec![0x1b]),
        "Shift+Tab" => return Some(b"\x1b[Z".to_vec()),
        "Ctrl+C" => return Some(vec![0x03]),
        "Ctrl+D" => return Some(vec![0x04]),
        "Ctrl+L" => return Some(vec![0x0c]),
        _ => {}
    }

    if let Some(command) = first.strip_prefix('/') {
        if command.is_empty()
            || !command
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return None;
        }
        let mut bytes = first.as_bytes().to_vec();
        bytes.push(b'\r');
        return Some(bytes);
    }

    // The application draws these as `Ctrl+N`, but a page is written by people and by
    // other tools; the modifier's spelling is not the point of the binding.
    if let Some(letter) = strip_prefix_either_case(first, "Ctrl+")
        .filter(|rest| rest.chars().count() == 1)
        .and_then(|rest| rest.chars().next())
    {
        let upper = letter.to_ascii_uppercase();
        if upper.is_ascii_uppercase() {
            // A is 0x01 through Z is 0x1A.
            return Some(vec![upper as u8 - b'A' + 1]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::keystrokes;

    #[test]
    fn a_control_letter_is_its_control_character() {
        assert_eq!(keystrokes("Ctrl+N"), Some(vec![0x0e]));
        assert_eq!(keystrokes("Ctrl+P"), Some(vec![0x10]));
        assert_eq!(keystrokes("ctrl+a"), Some(vec![0x01]));
    }

    #[test]
    fn a_slash_command_is_typed_and_returned() {
        assert_eq!(keystrokes("/dashboard"), Some(b"/dashboard\r".to_vec()));
    }

    #[test]
    fn shift_tab_is_the_sequence_a_terminal_sends() {
        assert_eq!(keystrokes("Shift+Tab"), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn a_destination_after_an_arrow_is_not_part_of_the_keys() {
        // `Ctrl+P → worktree` means press Ctrl+P; where the program then puts the person
        // is the program's decision.
        assert_eq!(keystrokes("Ctrl+P → worktree"), Some(vec![0x10]));
    }

    #[test]
    fn an_unknown_binding_is_refused_rather_than_guessed() {
        // A wrong guess types garbage into a running program.
        assert_eq!(keystrokes("F7"), None);
        assert_eq!(keystrokes("Ctrl+Shift+L"), None);
        assert_eq!(keystrokes(""), None);
        assert_eq!(keystrokes("/not a command"), None);
    }
}
