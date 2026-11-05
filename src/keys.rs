//! Translation of egui key events into the key type of the binding manager.
//!
//! A character key is read by its code, its place on the keyboard: a binding
//! is the same key on every layout. A key that types no character is read as
//! what it says, so a key swapped in the system keeps its meaning.

use zyt_keymux::{KeyCode, KeyStroke, Modifiers};

/// The key a binding reads: its code when it is a character key and has one.
pub fn code_of(key: egui::Key, physical: Option<egui::Key>) -> egui::Key {
    match character(key) {
        Some(_) => physical.unwrap_or(key),
        None => key,
    }
}

/// The character of a character key, in lower case.
fn character(key: egui::Key) -> Option<char> {
    match key {
        egui::Key::Minus => return Some('-'),
        egui::Key::Quote => return Some('\''),
        _ => {}
    }
    let mut chars = key.symbol_or_name().chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => Some(ch.to_ascii_lowercase()),
        _ => None,
    }
}

/// Key press for the binding manager, when the key has a text form. The key is
/// the one of [`code_of`].
pub fn to_stroke(key: egui::Key, modifiers: &egui::Modifiers) -> Option<KeyStroke> {
    let code = match key {
        egui::Key::Enter => KeyCode::Enter,
        egui::Key::Escape => KeyCode::Escape,
        egui::Key::Tab => KeyCode::Tab,
        egui::Key::Space => KeyCode::Space,
        egui::Key::Backspace => KeyCode::Backspace,
        egui::Key::Delete => KeyCode::Delete,
        egui::Key::Insert => KeyCode::Insert,
        egui::Key::Home => KeyCode::Home,
        egui::Key::End => KeyCode::End,
        egui::Key::PageUp => KeyCode::PageUp,
        egui::Key::PageDown => KeyCode::PageDown,
        egui::Key::ArrowUp => KeyCode::Up,
        egui::Key::ArrowDown => KeyCode::Down,
        egui::Key::ArrowLeft => KeyCode::Left,
        egui::Key::ArrowRight => KeyCode::Right,
        egui::Key::F1 => KeyCode::Function(1),
        egui::Key::F2 => KeyCode::Function(2),
        egui::Key::F3 => KeyCode::Function(3),
        egui::Key::F4 => KeyCode::Function(4),
        egui::Key::F5 => KeyCode::Function(5),
        egui::Key::F6 => KeyCode::Function(6),
        egui::Key::F7 => KeyCode::Function(7),
        egui::Key::F8 => KeyCode::Function(8),
        egui::Key::F9 => KeyCode::Function(9),
        egui::Key::F10 => KeyCode::Function(10),
        egui::Key::F11 => KeyCode::Function(11),
        egui::Key::F12 => KeyCode::Function(12),
        other => KeyCode::Char(character(other)?),
    };

    Some(
        KeyStroke {
            code,
            modifiers: Modifiers {
                shift: modifiers.shift,
                ctrl: modifiers.ctrl || modifiers.command,
                alt: modifiers.alt,
                meta: false,
            },
        }
        .normal(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(key: egui::Key, physical: egui::Key, modifiers: egui::Modifiers) -> String {
        to_stroke(code_of(key, Some(physical)), &modifiers)
            .expect("the key has a text form")
            .to_text()
    }

    #[test]
    fn a_character_key_is_read_by_its_place_on_any_layout() {
        let shift = egui::Modifiers::SHIFT;
        let none = egui::Modifiers::NONE;
        let ctrl = egui::Modifiers::CTRL;
        let (comma, semicolon) = (egui::Key::Comma, egui::Key::Semicolon);
        assert_eq!(stroke(comma, egui::Key::Slash, shift), "?");
        assert_eq!(stroke(semicolon, egui::Key::Num4, shift), "$");
        assert_eq!(stroke(comma, comma, none), ",");
        assert_eq!(stroke(egui::Key::Quote, egui::Key::Quote, shift), "\"");
        assert_eq!(stroke(egui::Key::C, egui::Key::I, none), "i");
        assert_eq!(stroke(egui::Key::B, egui::Key::B, ctrl), "ctrl+b");
        assert_eq!(
            stroke(egui::Key::P, egui::Key::P, ctrl | shift),
            "ctrl+shift+p"
        );
    }

    #[test]
    fn a_key_with_no_character_says_what_it_is() {
        let key = code_of(egui::Key::Escape, Some(egui::Key::A));
        assert_eq!(key, egui::Key::Escape);
    }

    #[test]
    fn a_key_with_no_code_is_read_as_it_comes() {
        assert_eq!(code_of(egui::Key::C, None), egui::Key::C);
    }
}
