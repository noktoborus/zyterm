//! Translation of key and mouse events into the bytes a terminal expects.
//! The types are defined here so the crate stays independent of any toolkit.

use crate::content::TerminalModes;

/// What the terminal sends when `Shift` is held with `Enter`.
///
/// The key says nothing of its own: there is no code for it, and a terminal
/// that sends what `Enter` sends is a terminal where a program cannot tell the
/// two apart. Programs that want a line of their own out of it — an editor of
/// one field, a prompt that takes several lines — read one of the two spellings
/// below, and which of them is the reader's answer because no program is
/// asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShiftEnter {
    /// What `Enter` sends, a carriage return. The shift is not reported at all.
    #[default]
    Enter,
    /// The escape and a carriage return, which is what `Alt` with a key sends.
    ///
    /// It is the oldest of the three and the one most programs already read,
    /// because it is what a terminal with no key reporting of its own can say.
    EscapeEnter,
    /// `CSI 13;2u`, the key and its modifier, of the `CSI u` keyboard
    /// protocol.
    ///
    /// It says which key was pressed and what was held with it, so a program
    /// that reads it tells `Shift+Enter` from `Alt+Enter`. A program that does
    /// not know the protocol reads it as a sequence it has no answer for and
    /// may print what is left of it.
    CsiU,
}

impl ShiftEnter {
    /// The bytes of it.
    fn bytes(self) -> Vec<u8> {
        match self {
            Self::Enter => vec![b'\r'],
            Self::EscapeEnter => vec![0x1b, b'\r'],
            Self::CsiU => b"\x1b[13;2u".to_vec(),
        }
    }
}

/// Key of a key press, toolkit independent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// A character produced by the keyboard layout.
    Char(char),
    /// Enter or Return.
    Enter,
    /// Backspace.
    Backspace,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
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
    /// Function key, one based.
    Function(u8),
}

/// Modifier keys held during an event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift is held.
    pub shift: bool,
    /// Control is held.
    pub ctrl: bool,
    /// Alt is held.
    pub alt: bool,
}

impl Modifiers {
    /// Parameter used by the modified escape sequences.
    fn code(self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.ctrl)
    }

    fn any(self) -> bool {
        self.shift || self.ctrl || self.alt
    }
}

/// Mouse button of a mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Left button.
    Left,
    /// Middle button.
    Middle,
    /// Right button.
    Right,
    /// Wheel up.
    WheelUp,
    /// Wheel down.
    WheelDown,
    /// The pointer moved with the left button held.
    LeftMove,
    /// The pointer moved with the middle button held.
    MiddleMove,
    /// The pointer moved with the right button held.
    RightMove,
    /// The pointer moved with no button held.
    NoneMove,
}

/// Encodes a key press. Returns nothing when the key produces no bytes.
///
/// `shift_enter` is what `Shift` with `Enter` sends, which is the caller's
/// answer: no program asks for it and every terminal spells it differently.
pub fn encode_key(
    key: &Key,
    modifiers: Modifiers,
    modes: TerminalModes,
    shift_enter: ShiftEnter,
) -> Option<Vec<u8>> {
    let bytes = match key {
        Key::Char(ch) => return encode_char(*ch, modifiers),
        Key::Enter if modifiers.shift && !modifiers.ctrl && !modifiers.alt => shift_enter.bytes(),
        Key::Enter => vec![b'\r'],
        Key::Backspace => {
            if modifiers.ctrl {
                vec![0x08]
            } else {
                vec![0x7f]
            }
        }
        Key::Tab => {
            if modifiers.shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Key::Escape => vec![0x1b],
        Key::Up => cursor_key(b'A', modifiers, modes),
        Key::Down => cursor_key(b'B', modifiers, modes),
        Key::Right => cursor_key(b'C', modifiers, modes),
        Key::Left => cursor_key(b'D', modifiers, modes),
        Key::Home => cursor_key(b'H', modifiers, modes),
        Key::End => cursor_key(b'F', modifiers, modes),
        Key::Insert => tilde_key(2, modifiers),
        Key::Delete => tilde_key(3, modifiers),
        Key::PageUp => tilde_key(5, modifiers),
        Key::PageDown => tilde_key(6, modifiers),
        Key::Function(number) => function_key(*number, modifiers)?,
    };

    if modifiers.alt && matches!(key, Key::Enter | Key::Backspace | Key::Escape) {
        let mut prefixed = vec![0x1b];
        prefixed.extend_from_slice(&bytes);
        return Some(prefixed);
    }
    Some(bytes)
}

/// Encodes text coming from a paste, honouring bracketed paste mode.
pub fn encode_paste(text: &str, modes: TerminalModes) -> Vec<u8> {
    let cleaned: String = text.replace("\r\n", "\r").replace('\n', "\r");
    if !modes.bracketed_paste {
        return cleaned.into_bytes();
    }
    let mut bytes = b"\x1b[200~".to_vec();
    bytes.extend_from_slice(cleaned.as_bytes());
    bytes.extend_from_slice(b"\x1b[201~");
    bytes
}

/// Encodes a mouse event when the program asked for mouse reports.
pub fn encode_mouse(
    button: MouseButton,
    pressed: bool,
    column: usize,
    row: usize,
    modifiers: Modifiers,
    modes: TerminalModes,
) -> Option<Vec<u8>> {
    if !modes.mouse_report {
        return None;
    }
    let mut code = match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
        MouseButton::LeftMove => 32,
        MouseButton::MiddleMove => 33,
        MouseButton::RightMove => 34,
        MouseButton::NoneMove => 35,
    };
    if modifiers.shift {
        code += 4;
    }
    if modifiers.alt {
        code += 8;
    }
    if modifiers.ctrl {
        code += 16;
    }

    let column = column + 1;
    let row = row + 1;
    if modes.sgr_mouse {
        let kind = if pressed { 'M' } else { 'm' };
        return Some(format!("\x1b[<{code};{column};{row}{kind}").into_bytes());
    }

    if column > 223 || row > 223 {
        return None;
    }
    let motion = matches!(
        button,
        MouseButton::LeftMove
            | MouseButton::MiddleMove
            | MouseButton::RightMove
            | MouseButton::NoneMove
    );
    let code = if pressed || motion { code } else { 3 };
    Some(vec![
        0x1b,
        b'[',
        b'M',
        32 + code,
        32 + column as u8,
        32 + row as u8,
    ])
}

fn encode_char(ch: char, modifiers: Modifiers) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    if modifiers.ctrl {
        let control = match ch {
            ' ' | '@' => Some(0x00),
            'a'..='z' => Some(ch as u8 - b'a' + 1),
            'A'..='Z' => Some(ch as u8 - b'A' + 1),
            '[' => Some(0x1b),
            '\\' => Some(0x1c),
            ']' => Some(0x1d),
            '^' => Some(0x1e),
            '_' => Some(0x1f),
            '?' => Some(0x7f),
            _ => None,
        };
        if let Some(control) = control {
            if modifiers.alt {
                bytes.push(0x1b);
            }
            bytes.push(control);
            return Some(bytes);
        }
    }
    if modifiers.alt {
        bytes.push(0x1b);
    }
    let mut buffer = [0u8; 4];
    bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
    Some(bytes)
}

fn cursor_key(final_byte: u8, modifiers: Modifiers, modes: TerminalModes) -> Vec<u8> {
    if modifiers.any() {
        return format!("\x1b[1;{}{}", modifiers.code(), final_byte as char).into_bytes();
    }
    if modes.app_cursor {
        vec![0x1b, b'O', final_byte]
    } else {
        vec![0x1b, b'[', final_byte]
    }
}

fn tilde_key(number: u8, modifiers: Modifiers) -> Vec<u8> {
    if modifiers.any() {
        format!("\x1b[{};{}~", number, modifiers.code()).into_bytes()
    } else {
        format!("\x1b[{number}~").into_bytes()
    }
}

fn function_key(number: u8, modifiers: Modifiers) -> Option<Vec<u8>> {
    let sequence = match number {
        1 => return Some(modified_ss3(b'P', modifiers)),
        2 => return Some(modified_ss3(b'Q', modifiers)),
        3 => return Some(modified_ss3(b'R', modifiers)),
        4 => return Some(modified_ss3(b'S', modifiers)),
        5 => 15,
        6 => 17,
        7 => 18,
        8 => 19,
        9 => 20,
        10 => 21,
        11 => 23,
        12 => 24,
        _ => return None,
    };
    Some(tilde_key(sequence, modifiers))
}

fn modified_ss3(final_byte: u8, modifiers: Modifiers) -> Vec<u8> {
    if modifiers.any() {
        format!("\x1b[1;{}{}", modifiers.code(), final_byte as char).into_bytes()
    } else {
        vec![0x1b, b'O', final_byte]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes() -> TerminalModes {
        TerminalModes::default()
    }

    /// `Shift` with `Enter` is spelled the way the caller says, and nothing
    /// else about the key changes.
    #[test]
    fn shift_with_enter_is_spelled_the_way_the_caller_says() {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        for (spelling, bytes) in [
            (ShiftEnter::Enter, b"\r".to_vec()),
            (ShiftEnter::EscapeEnter, b"\x1b\r".to_vec()),
            (ShiftEnter::CsiU, b"\x1b[13;2u".to_vec()),
        ] {
            assert_eq!(
                encode_key(&Key::Enter, shift, modes(), spelling),
                Some(bytes),
                "{spelling:?}"
            );
            assert_eq!(
                encode_key(&Key::Enter, Modifiers::default(), modes(), spelling),
                Some(b"\r".to_vec()),
                "the key alone is a carriage return whatever the setting"
            );
            assert_eq!(
                encode_key(
                    &Key::Enter,
                    Modifiers {
                        ctrl: true,
                        ..Modifiers::default()
                    },
                    modes(),
                    spelling
                ),
                Some(b"\r".to_vec()),
                "and so is the key with anything else held"
            );
        }
    }

    #[test]
    fn control_letters_become_control_codes() {
        let bytes = encode_key(
            &Key::Char('c'),
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            modes(),
            ShiftEnter::default(),
        )
        .expect("ctrl+c produces bytes");
        assert_eq!(bytes, vec![0x03]);
    }

    #[test]
    fn cursor_keys_follow_the_application_mode() {
        assert_eq!(
            encode_key(
                &Key::Up,
                Modifiers::default(),
                modes(),
                ShiftEnter::default()
            )
            .unwrap(),
            b"\x1b[A".to_vec()
        );
        let app = TerminalModes {
            app_cursor: true,
            ..TerminalModes::default()
        };
        assert_eq!(
            encode_key(&Key::Up, Modifiers::default(), app, ShiftEnter::default()).unwrap(),
            b"\x1bOA".to_vec()
        );
    }

    #[test]
    fn motion_is_reported_with_its_own_codes() {
        let modes = TerminalModes {
            mouse_report: true,
            sgr_mouse: true,
            mouse_drag: true,
            ..TerminalModes::default()
        };

        let bytes = encode_mouse(
            MouseButton::LeftMove,
            true,
            4,
            2,
            Modifiers::default(),
            modes,
        )
        .expect("motion is encoded");
        assert_eq!(bytes, b"\x1b[<32;5;3M".to_vec());

        let bytes = encode_mouse(
            MouseButton::NoneMove,
            true,
            0,
            0,
            Modifiers::default(),
            modes,
        )
        .expect("motion is encoded");
        assert_eq!(bytes, b"\x1b[<35;1;1M".to_vec());
    }

    #[test]
    fn paste_is_bracketed_when_requested() {
        let plain = encode_paste("a\r\nb", modes());
        assert_eq!(plain, b"a\rb".to_vec());
        let bracketed = TerminalModes {
            bracketed_paste: true,
            ..TerminalModes::default()
        };
        assert_eq!(
            encode_paste("a", bracketed),
            b"\x1b[200~a\x1b[201~".to_vec()
        );
    }
}
