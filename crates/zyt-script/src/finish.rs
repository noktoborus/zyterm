//! The key sent once a run is over.

use crate::error::{Result, ScriptError};

/// What a script writes when it sends no key at all.
pub const FINISH_NONE: &str = "";

/// The keys worth offering in a list, in the order they are offered.
pub const FINISH_PRESETS: &[&str] = &["esc", "ctrl+c", "ctrl+d", "enter"];

/// Turns the text form of a key into the bytes sent to the device.
///
/// Accepts the names `esc`, `escape`, `enter`, `cr`, `lf`, `tab`, the form
/// `ctrl+<char>`, the escapes `\r`, `\n`, `\t`, `\e`, `\\` and `\xNN`,
/// and takes anything else as literal text.
pub fn finish_bytes(text: &str) -> Result<Vec<u8>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let lower = trimmed.to_ascii_lowercase();
    let named = match lower.as_str() {
        "esc" | "escape" => Some(0x1b),
        "enter" | "return" | "cr" => Some(0x0d),
        "lf" | "newline" => Some(0x0a),
        "tab" => Some(0x09),
        "backspace" => Some(0x7f),
        _ => None,
    };
    if let Some(byte) = named {
        return Ok(vec![byte]);
    }

    if let Some(rest) = lower.strip_prefix("ctrl+") {
        let mut characters = rest.chars();
        let (Some(character), None) = (characters.next(), characters.next()) else {
            return Err(ScriptError::InvalidFinishKey {
                input: trimmed.to_string(),
            });
        };
        return control_byte(character)
            .map(|byte| vec![byte])
            .ok_or_else(|| ScriptError::InvalidFinishKey {
                input: trimmed.to_string(),
            });
    }

    unescape(trimmed)
}

/// Text of that key for a message, when there is one.
pub fn finish_label(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Control code of one character, as a terminal sends it with Control held.
fn control_byte(character: char) -> Option<u8> {
    match character {
        'a'..='z' => Some(character as u8 - b'a' + 1),
        '@' | ' ' => Some(0x00),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        '?' => Some(0x7f),
        _ => None,
    }
}

/// Resolves the backslash escapes of a literal key.
fn unescape(text: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut characters = text.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            let mut buffer = [0u8; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            continue;
        }

        let invalid = || ScriptError::InvalidFinishKey {
            input: text.to_string(),
        };
        match characters.next().ok_or_else(invalid)? {
            'r' => bytes.push(0x0d),
            'n' => bytes.push(0x0a),
            't' => bytes.push(0x09),
            'e' => bytes.push(0x1b),
            '0' => bytes.push(0x00),
            '\\' => bytes.push(b'\\'),
            'x' => {
                let high = characters.next().ok_or_else(invalid)?;
                let low = characters.next().ok_or_else(invalid)?;
                let digits: String = [high, low].into_iter().collect();
                let byte = u8::from_str_radix(&digits, 16).map_err(|_| invalid())?;
                bytes.push(byte);
            }
            _ => return Err(invalid()),
        }
    }

    Ok(bytes)
}
