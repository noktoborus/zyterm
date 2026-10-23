//! The names tmux knows keys by, for the keys tmux encodes itself.
//!
//! tmux encodes a key typed in a client for the terminal of the pane: `Home`
//! is `\E[1~` whatever the cursor key mode, which is what the terminfo of
//! `tmux-256color` and `screen` says, where this terminal says `\EOH`. Sent as bytes by `send-keys -H` it reaches the pane as it was
//! typed, and a program that reads the terminfo reads `ESC`, `O`, `H`. So the
//! keys whose encoding is a matter of the terminal go by name; text, control
//! bytes and the keys every terminal agrees on go as bytes.

use zyt_term::{Key, Modifiers};

/// The highest function key tmux has a name for.
const LAST_FUNCTION: u8 = 12;

/// The name of `key` held with `modifiers` in tmux — `Home`, `C-Up`, `S-F5` —
/// or `None` for a key that goes as bytes.
pub fn name(key: &Key, modifiers: Modifiers) -> Option<String> {
    let base = match key {
        Key::Up => "Up".to_string(),
        Key::Down => "Down".to_string(),
        Key::Left => "Left".to_string(),
        Key::Right => "Right".to_string(),
        Key::Home => "Home".to_string(),
        Key::End => "End".to_string(),
        Key::PageUp => "PPage".to_string(),
        Key::PageDown => "NPage".to_string(),
        Key::Insert => "IC".to_string(),
        Key::Delete => "DC".to_string(),
        Key::Function(number @ 1..=LAST_FUNCTION) => format!("F{number}"),
        _ => return None,
    };
    let mut name = String::new();
    if modifiers.ctrl {
        name.push_str("C-");
    }
    if modifiers.alt {
        name.push_str("M-");
    }
    if modifiers.shift {
        name.push_str("S-");
    }
    name.push_str(&base);
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    #[test]
    fn the_keys_of_the_terminal_go_by_name() {
        assert_eq!(name(&Key::Home, NONE).as_deref(), Some("Home"));
        assert_eq!(name(&Key::End, NONE).as_deref(), Some("End"));
        assert_eq!(name(&Key::PageUp, NONE).as_deref(), Some("PPage"));
        assert_eq!(name(&Key::Delete, NONE).as_deref(), Some("DC"));
        assert_eq!(name(&Key::Function(5), NONE).as_deref(), Some("F5"));
    }

    #[test]
    fn modifiers_are_prefixes() {
        let all = Modifiers {
            shift: true,
            ctrl: true,
            alt: true,
        };
        assert_eq!(name(&Key::Up, all).as_deref(), Some("C-M-S-Up"));
    }

    #[test]
    fn text_and_the_keys_every_terminal_agrees_on_go_as_bytes() {
        for key in [
            Key::Char('a'),
            Key::Enter,
            Key::Tab,
            Key::Backspace,
            Key::Escape,
            Key::Function(13),
        ] {
            assert_eq!(name(&key, NONE), None, "{key:?}");
        }
    }
}
