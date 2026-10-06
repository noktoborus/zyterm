//! `zyt.codec`: the work that costs, done where it is cheap.
//!
//! base64 over a file and a sum over a file are the two things a transfer does
//! that cost processor time rather than waiting, and a script that did them a
//! byte at a time would be slower than the line it is feeding. They are here,
//! and the geometry base64 travels in is here with them: 57 bytes make a line
//! of 76 characters, which is what `base64` itself writes and what a terminal
//! in its usual mode carries without dropping the end of it.

use super::external;
use crate::error::ScriptError;
use base64::Engine;
use md5::Digest as Summing;
use mlua::{Lua, Table};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How many bytes of a file make one line of base64.
pub(crate) const LINE_BYTES: usize = 57;

/// How many characters that line is, before the newline that ends it.
pub(crate) const LINE_CHARS: usize = 76;

/// How much of a file is read at once while a sum is taken of it.
const SUMMING: usize = 64 * 1024;

/// Builds the table.
pub(crate) fn table(lua: &Lua) -> mlua::Result<Table> {
    let codec = lua.create_table()?;

    codec.set(
        "b64_encode",
        lua.create_function(|lua, data: mlua::LuaString| {
            lua.create_string(encode(&data.as_bytes()))
        })?,
    )?;

    codec.set(
        "b64_decode",
        lua.create_function(|lua, text: String| {
            let bytes = decode(&text).map_err(external)?;
            lua.create_string(&bytes)
        })?,
    )?;

    codec.set(
        "b64_slice",
        lua.create_function(|lua, (path, offset, count): (String, u64, u64)| {
            let bytes = slice(Path::new(&path), offset, count).map_err(external)?;
            lua.create_string(encode(&bytes))
        })?,
    )?;

    codec.set(
        "slice",
        lua.create_function(|lua, (path, offset, count): (String, u64, u64)| {
            let bytes = slice(Path::new(&path), offset, count).map_err(external)?;
            lua.create_string(&bytes)
        })?,
    )?;

    codec.set(
        "lines_of",
        lua.create_function(|_, on_wire: usize| Ok(lines_of(on_wire)))?,
    )?;

    codec.set(
        "digest",
        lua.create_function(|_, (path, algorithm): (String, String)| {
            digest_file(Path::new(&path), &algorithm).map_err(external)
        })?,
    )?;

    codec.set(
        "digest_data",
        lua.create_function(|_, (data, algorithm): (mlua::LuaString, String)| {
            digest_bytes(&data.as_bytes(), &algorithm).map_err(external)
        })?,
    )?;

    codec.set(
        "short_size",
        lua.create_function(|_, bytes: u64| Ok(short_size(bytes)))?,
    )?;

    Ok(codec)
}

/// The bytes as base64, cut into the lines the wire takes.
pub(crate) fn encode(bytes: &[u8]) -> Vec<u8> {
    let lines = bytes.len().div_ceil(LINE_BYTES);
    let mut text = String::with_capacity(lines * (LINE_CHARS + 1));
    for piece in bytes.chunks(LINE_BYTES) {
        text.push_str(&base64::engine::general_purpose::STANDARD.encode(piece));
        text.push('\n');
    }
    text.into_bytes()
}

/// The bytes one line of base64 stands for.
pub(crate) fn decode(text: &str) -> crate::Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|source| ScriptError::Decode { source })
}

/// How many bytes of a file fit in a chunk of that many bytes on the wire.
///
/// At least one line, because a chunk smaller than a line still has to carry
/// something, and whole lines only: half a line of base64 decodes to nothing
/// the device can place.
pub(crate) fn lines_of(on_wire: usize) -> usize {
    (on_wire / (LINE_CHARS + 1)).max(1) * LINE_BYTES
}

/// That many bytes of the file, starting at that offset counted from one.
///
/// From one because that is what `tail -c +N` counts in, and the offset of a
/// chunk is written into the command that asks the device for it: the two ends
/// of a transfer count the same way or they do not meet.
pub(crate) fn slice(path: &Path, offset: u64, count: u64) -> crate::Result<Vec<u8>> {
    let failed = |source: std::io::Error| ScriptError::Io {
        path: path.to_path_buf(),
        source,
    };

    let mut file = std::fs::File::open(path).map_err(failed)?;
    file.seek(SeekFrom::Start(offset.saturating_sub(1)))
        .map_err(failed)?;

    let mut bytes = vec![0u8; count as usize];
    let mut filled = 0;
    while filled < bytes.len() {
        let read = file.read(&mut bytes[filled..]).map_err(failed)?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    bytes.truncate(filled);
    Ok(bytes)
}

/// The sum of a file, in lower case hexadecimal.
pub(crate) fn digest_file(path: &Path, algorithm: &str) -> crate::Result<String> {
    let mut file = std::fs::File::open(path).map_err(|source| ScriptError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let mut buffer = vec![0u8; SUMMING];
    let mut summing = summing_of(algorithm, path)?;
    loop {
        let read = file.read(&mut buffer).map_err(|source| ScriptError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        summing.feed(&buffer[..read]);
    }
    Ok(summing.finish())
}

/// The sum of what is in hand, in lower case hexadecimal.
pub(crate) fn digest_bytes(bytes: &[u8], algorithm: &str) -> crate::Result<String> {
    let mut summing = summing_of(algorithm, Path::new(""))?;
    summing.feed(bytes);
    Ok(summing.finish())
}

/// One of the three sums, by the name the device knows it under.
fn summing_of(algorithm: &str, path: &Path) -> crate::Result<Summer> {
    match algorithm {
        "md5" | "md5sum" => Ok(Summer::Md5(md5::Md5::new())),
        "sha1" | "sha1sum" => Ok(Summer::Sha1(sha1::Sha1::new())),
        "sha256" | "sha256sum" => Ok(Summer::Sha256(sha2::Sha256::new())),
        _ => Err(ScriptError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(format!("no sum is called {algorithm}")),
        }),
    }
}

/// One sum being taken.
enum Summer {
    /// The weakest of the three, and the one every device has.
    Md5(md5::Md5),
    /// The one in between.
    Sha1(sha1::Sha1),
    /// The strongest.
    Sha256(sha2::Sha256),
}

impl Summer {
    /// Takes in more of the file.
    fn feed(&mut self, bytes: &[u8]) {
        match self {
            Self::Md5(summing) => summing.update(bytes),
            Self::Sha1(summing) => summing.update(bytes),
            Self::Sha256(summing) => summing.update(bytes),
        }
    }

    /// The sum, in the lower case hexadecimal every one of these programs
    /// writes.
    fn finish(self) -> String {
        let bytes: Vec<u8> = match self {
            Self::Md5(summing) => summing.finalize().to_vec(),
            Self::Sha1(summing) => summing.finalize().to_vec(),
            Self::Sha256(summing) => summing.finalize().to_vec(),
        };
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// A size in the shortest form that still says it.
pub(crate) fn short_size(bytes: u64) -> String {
    const STEP: u64 = 1024;
    let units = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= STEP as f64 && unit + 1 < units.len() {
        value /= STEP as f64;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", units[0])
    } else {
        format!("{value:.1}{}", units[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_line_of_base64_is_longer_than_a_terminal_carries() {
        let bytes: Vec<u8> = (0..40_000).map(|at| (at % 251) as u8).collect();
        let text = String::from_utf8(encode(&bytes)).expect("it is text");
        for line in text.lines() {
            assert!(line.len() <= LINE_CHARS, "{} characters", line.len());
        }
    }

    #[test]
    fn what_was_encoded_comes_back_line_by_line() {
        let bytes: Vec<u8> = (0..=255).collect();
        let text = String::from_utf8(encode(&bytes)).expect("it is text");
        let mut back = Vec::new();
        for line in text.lines() {
            back.extend_from_slice(&decode(line).expect("the line decodes"));
        }
        assert_eq!(back, bytes);
    }

    #[test]
    fn a_chunk_of_the_wire_is_whole_lines_of_the_file() {
        assert_eq!(lines_of(2048), 26 * LINE_BYTES);
        assert_eq!(lines_of(77), LINE_BYTES);
        assert_eq!(lines_of(1), LINE_BYTES, "a chunk carries at least a line");
    }

    #[test]
    fn a_slice_counts_from_one_and_stops_at_the_end_of_the_file() {
        let path = std::env::temp_dir().join(format!("zyt-script-slice-{}", std::process::id()));
        std::fs::write(&path, b"0123456789").expect("the file is written");

        assert_eq!(slice(&path, 1, 4).expect("it reads"), b"0123");
        assert_eq!(slice(&path, 5, 4).expect("it reads"), b"4567");
        assert_eq!(
            slice(&path, 9, 100).expect("it reads"),
            b"89",
            "a slice past the end is as long as what is there"
        );

        std::fs::remove_file(&path).expect("it is taken away");
    }

    #[test]
    fn the_three_sums_are_the_ones_the_device_names() {
        assert_eq!(
            digest_bytes(b"x", "md5").expect("md5"),
            "9dd4e461268c8034f5c8564e155c67a6"
        );
        assert_eq!(
            digest_bytes(b"x", "sha1sum").expect("sha1"),
            "11f6ad8ec52a2984abaafd7c3b516503785c2072"
        );
        assert_eq!(
            digest_bytes(b"x", "sha256").expect("sha256"),
            "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"
        );
        assert!(digest_bytes(b"x", "crc32").is_err());
    }

    #[test]
    fn a_size_is_as_short_as_it_can_be() {
        assert_eq!(short_size(0), "0 B");
        assert_eq!(short_size(512), "512 B");
        assert_eq!(short_size(4096), "4.0K");
        assert_eq!(short_size(1_258_291), "1.2M");
    }
}
