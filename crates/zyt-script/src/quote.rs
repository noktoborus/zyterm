//! Quoting a value so a shell reads it as one word.

/// Quotes a value for the shell of this machine, so a path with a space or a
/// quote in it stays one word.
pub fn quote_for_shell(value: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        quote_posix(value)
    }
}

/// Quotes a value the way a POSIX shell reads it, whatever this machine runs.
///
/// It is the form the device is sent: the shell at the far end of a line is
/// `sh` even when the program driving it runs on Windows, so the two cases are
/// separate calls rather than one that guesses from the platform.
pub fn quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quote_in_a_value_is_closed_escaped_and_opened_again() {
        assert_eq!(quote_posix("it's"), r"'it'\''s'");
    }

    #[test]
    fn a_space_keeps_the_value_one_word() {
        assert_eq!(quote_posix("/tmp/two words"), "'/tmp/two words'");
    }

    #[test]
    fn the_shell_of_the_device_is_quoted_the_posix_way_on_every_platform() {
        assert!(quote_posix("x").starts_with('\''));
    }
}
