//! Translation of egui key events into the key type of the binding manager.

use zyt_keymux::{KeyCode, KeyStroke, Modifiers};

/// Key press for the binding manager, when the key has a text form.
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
        other => {
            let name = other.name();
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => KeyCode::Char(ch.to_ascii_lowercase()),
                _ => return None,
            }
        }
    };

    Some(KeyStroke {
        code,
        modifiers: Modifiers {
            shift: modifiers.shift,
            ctrl: modifiers.ctrl || modifiers.command,
            alt: modifiers.alt,
            meta: false,
        },
    })
}
