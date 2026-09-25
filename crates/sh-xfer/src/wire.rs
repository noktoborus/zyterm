//! The reply line of the protocol, and quoting for the shell that answers it.
//!
//! Every answer of the device ends in a line carrying [`MARK`] and three
//! digits. It is looked for anywhere in the line, not at the start of it: a
//! shell writes its prompt without a newline after it, so the first line of an
//! answer arrives as `# ### 100` and an answer anchored to the start of a line
//! would never be seen.
//!
//! That is safe only because the client never sends the marker itself. A
//! console echoes back everything written to it, so a script saying
//! `echo '### 200'` would come back looking exactly like the answer it asks
//! for. The scripts therefore write it in two pieces — `'##''# 200'`, which is
//! `### 200` to the shell and nothing of the sort to anything reading the
//! line.

/// What a reply line carries before its code.
pub const MARK: &str = "###";

/// What a reply line says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// Success, when nothing came before it.
    Done,
    /// The device is ready for what the client sends next.
    Ready,
    /// The answer follows, up to the next reply line.
    Data,
    /// The answer ended.
    End,
    /// The device refused.
    Failed,
    /// A code this protocol does not define, kept as it arrived so a
    /// complaint can name it.
    Other(u32),
}

impl Reply {
    /// The three digits the device writes for this reply.
    pub fn code(self) -> u32 {
        match self {
            Self::Done => 0,
            Self::Ready => 1,
            Self::Data => 100,
            Self::End => 200,
            Self::Failed => 500,
            Self::Other(code) => code,
        }
    }

    /// What a code off the line means.
    pub fn of_code(code: u32) -> Self {
        match code {
            0 => Self::Done,
            1 => Self::Ready,
            100 => Self::Data,
            200 => Self::End,
            500 => Self::Failed,
            other => Self::Other(other),
        }
    }

    /// The reply a line carries, when the line is one.
    pub fn of_line(line: &str) -> Option<Self> {
        let at = line.find(MARK)?;
        let digits: String = line[at + MARK.len()..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok().map(Self::of_code)
    }

    /// Whatever the device wrote after the code.
    ///
    /// The marker is found where it stands, not at the start of the line: a
    /// shell writes its prompt without a newline after it, and the first line
    /// of an answer is the one that carries a size.
    pub fn text_of_line(line: &str) -> String {
        let Some(at) = line.find(MARK) else {
            return String::new();
        };
        line[at + MARK.len()..]
            .trim_start()
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim()
            .to_string()
    }
}

/// Quotes a value so the shell of the device takes it as one word.
///
/// Single quotes hold everything but a single quote, which is closed, escaped
/// and opened again — the one form every POSIX shell reads the same way.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_line_is_read_by_its_code() {
        assert_eq!(Reply::of_line("### 200"), Some(Reply::End));
        assert_eq!(Reply::of_line("### 000 all done"), Some(Reply::Done));
        assert_eq!(Reply::of_line("###500"), Some(Reply::Failed));
        assert_eq!(Reply::of_line("  ### 100"), Some(Reply::Data));
        assert_eq!(Reply::of_line("### 001"), Some(Reply::Ready));
    }

    #[test]
    fn a_code_this_protocol_does_not_define_is_kept_as_it_arrived() {
        assert_eq!(Reply::of_line("### 999"), Some(Reply::Other(999)));
        assert_eq!(Reply::Other(999).code(), 999);
    }

    #[test]
    fn every_reply_survives_the_trip_through_its_code() {
        for reply in [
            Reply::Done,
            Reply::Ready,
            Reply::Data,
            Reply::End,
            Reply::Failed,
            Reply::Other(404),
        ] {
            assert_eq!(Reply::of_code(reply.code()), reply);
        }
    }

    #[test]
    fn a_prompt_glued_to_the_front_does_not_hide_a_reply() {
        assert_eq!(Reply::of_line("# ### 100"), Some(Reply::Data));
        assert_eq!(Reply::of_line("/root # ### 100 4096"), Some(Reply::Data));
        assert_eq!(Reply::of_line("[root@board ~]$ ### 000"), Some(Reply::Done));
    }

    #[test]
    fn everything_else_is_not_a_reply() {
        assert_eq!(Reply::of_line("### done"), None);
        assert_eq!(Reply::of_line("/tmp # "), None);
        assert_eq!(Reply::of_line(""), None);
    }

    #[test]
    fn what_the_device_wrote_after_the_code_is_kept() {
        assert_eq!(Reply::text_of_line("### 100 4096"), "4096");
        assert_eq!(Reply::text_of_line("/root # ### 100 4096"), "4096");
        assert_eq!(Reply::text_of_line("### 100"), "");
    }

    #[test]
    fn a_word_stays_one_word() {
        assert_eq!(quote("/tmp/two words"), "'/tmp/two words'");
        assert_eq!(quote("it's"), r"'it'\''s'");
    }
}
