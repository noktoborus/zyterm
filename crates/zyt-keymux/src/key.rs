//! Toolkit independent key description and its text form.

use crate::error::{KeymapError, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Key without modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// Character key, stored in lower case.
    Char(char),
    /// Function key, one based.
    Function(u8),
    /// Enter or Return.
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// Space.
    Space,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Insert.
    Insert,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
}

/// Modifier keys held during a key press.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift is held.
    pub shift: bool,
    /// Control is held.
    pub ctrl: bool,
    /// Alt is held.
    pub alt: bool,
    /// Command or Windows key is held.
    pub meta: bool,
}

/// One key press: a key and the modifiers held with it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyStroke {
    /// Key that was pressed.
    pub code: KeyCode,
    /// Modifiers held during the press.
    pub modifiers: Modifiers,
}

impl KeyStroke {
    /// Key press without modifiers.
    pub fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::default(),
        }
    }

    /// Parses one key press, for example `ctrl+shift+p`.
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(KeymapError::InvalidKey {
                input: input.to_string(),
            });
        }

        let mut modifiers = Modifiers::default();
        let parts: Vec<&str> = trimmed.split('+').collect();
        let (last, rest) = parts.split_last().ok_or_else(|| KeymapError::InvalidKey {
            input: input.to_string(),
        })?;

        for part in rest {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "shift" => modifiers.shift = true,
                "alt" | "option" => modifiers.alt = true,
                "meta" | "cmd" | "super" | "win" => modifiers.meta = true,
                other => {
                    return Err(KeymapError::UnknownModifier {
                        name: other.to_string(),
                    });
                }
            }
        }

        Ok(Self {
            code: parse_code(last, input)?,
            modifiers,
        })
    }

    /// Text form, the inverse of [`KeyStroke::parse`].
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        if self.modifiers.ctrl {
            text.push_str("ctrl+");
        }
        if self.modifiers.alt {
            text.push_str("alt+");
        }
        if self.modifiers.shift {
            text.push_str("shift+");
        }
        if self.modifiers.meta {
            text.push_str("meta+");
        }
        text.push_str(&code_text(&self.code));
        text
    }
}

impl fmt::Display for KeyStroke {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_text())
    }
}

/// Sequence of key presses that triggers one command.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Chord {
    /// Presses in order.
    pub strokes: Vec<KeyStroke>,
}

impl Chord {
    /// Parses a sequence such as `ctrl+k ctrl+s`.
    pub fn parse(input: &str) -> Result<Self> {
        let strokes = input
            .split_whitespace()
            .map(KeyStroke::parse)
            .collect::<Result<Vec<_>>>()?;
        if strokes.is_empty() {
            return Err(KeymapError::EmptyBinding);
        }
        Ok(Self { strokes })
    }

    /// Text form, the inverse of [`Chord::parse`].
    pub fn to_text(&self) -> String {
        self.strokes
            .iter()
            .map(KeyStroke::to_text)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// True when `prefix` is the beginning of this sequence.
    pub fn starts_with(&self, prefix: &[KeyStroke]) -> bool {
        prefix.len() <= self.strokes.len() && self.strokes[..prefix.len()] == *prefix
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_text())
    }
}

impl Serialize for Chord {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_text())
    }
}

impl<'de> Deserialize<'de> for Chord {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Chord::parse(&text).map_err(serde::de::Error::custom)
    }
}

fn parse_code(text: &str, input: &str) -> Result<KeyCode> {
    let lower = text.trim().to_ascii_lowercase();
    let code = match lower.as_str() {
        "enter" | "return" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Escape,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Space,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        other => {
            if let Some(number) = other.strip_prefix('f')
                && let Ok(number) = number.parse::<u8>()
                && (1..=24).contains(&number)
            {
                return Ok(KeyCode::Function(number));
            }
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => KeyCode::Char(ch),
                _ => {
                    return Err(KeymapError::InvalidKey {
                        input: input.to_string(),
                    });
                }
            }
        }
    };
    Ok(code)
}

fn code_text(code: &KeyCode) -> String {
    match code {
        KeyCode::Char(ch) => ch.to_string(),
        KeyCode::Function(number) => format!("f{number}"),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Escape => "escape".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Space => "space".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_round_trip() {
        let stroke = KeyStroke::parse("ctrl+shift+p").expect("valid key");
        assert_eq!(stroke.code, KeyCode::Char('p'));
        assert!(stroke.modifiers.ctrl && stroke.modifiers.shift);
        assert_eq!(stroke.to_text(), "ctrl+shift+p");
    }

    #[test]
    fn chord_round_trip() {
        let chord = Chord::parse("ctrl+k ctrl+s").expect("valid chord");
        assert_eq!(chord.strokes.len(), 2);
        assert_eq!(chord.to_text(), "ctrl+k ctrl+s");
    }

    #[test]
    fn unknown_modifier_is_reported() {
        let error = KeyStroke::parse("hyper+p").expect_err("modifier is unknown");
        assert!(matches!(error, KeymapError::UnknownModifier { .. }));
    }
}
