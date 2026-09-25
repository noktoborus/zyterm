//! Sniffer for the operating system commands the parser drops.
//!
//! The emulation backend handles the title, the colors, the hyperlinks and the
//! clipboard, but never reports the working directory, notifications or shell
//! marks. They are picked up here, beside the parser, over the same bytes: the
//! scan looks for the escape byte with `memchr`, so ordinary output costs one
//! pass and nothing else.

/// Longest sequence that is kept while it is still incomplete.
const MAX_PAYLOAD: usize = 4096;

/// Where a shell says it is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    /// The prompt starts here.
    PromptStart,
    /// The typed command starts here.
    CommandStart,
    /// The output of the command starts here.
    OutputStart,
    /// The command ended, with its exit code when it was reported.
    CommandEnd(Option<i32>),
}

/// Which sequence asked for a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    /// OSC 9: a text with no heading.
    Text,
    /// OSC 777: a heading and a text.
    Titled,
}

/// What a program says about the progress of what it is doing (OSC 9;4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressState {
    /// Nothing is running any more.
    Removed,
    /// So far of a hundred.
    Set(u8),
    /// Something went wrong, at the share it had reached.
    Error(u8),
    /// Something runs, but how far along is not known.
    Indeterminate,
    /// It stands still at the share it had reached.
    Paused(u8),
}

/// One sequence the sniffer recognized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscReport {
    /// The program reported its working directory (OSC 7).
    WorkingDirectory(String),
    /// The program asked for a desktop notification (OSC 9 or OSC 777).
    Notification {
        /// Sequence that asked for it.
        kind: NotificationKind,
        /// Heading of the notification.
        title: String,
        /// Text of the notification.
        body: String,
    },
    /// The shell marked a point of its cycle (OSC 133).
    Mark(MarkKind),
    /// The program reported how far along it is (OSC 9;4).
    Progress(ProgressState),
}

/// One report and where the sequence that carried it ended.
///
/// The offset is into the chunk that was fed, just past the terminator, so a
/// caller can drive its parser up to that point before it acts on the report.
/// A sequence split across chunks completes in the chunk that ends it, so the
/// offset always names a byte of the chunk in hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SniffedReport {
    /// Offset in the chunk just past the terminator of the sequence.
    pub end: usize,
    /// What the sequence said.
    pub report: OscReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Nothing seen yet.
    Idle,
    /// The escape byte was the last byte.
    Escape,
    /// Inside the payload of a sequence.
    Payload,
    /// The escape byte inside a payload, which may end it.
    PayloadEscape,
}

/// Collects the sequences the parser does not report.
#[derive(Debug)]
pub struct OscSniffer {
    state: State,
    payload: Vec<u8>,
    dropped: bool,
}

impl Default for OscSniffer {
    fn default() -> Self {
        Self::new()
    }
}

impl OscSniffer {
    /// Sniffer that has seen nothing.
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            payload: Vec::new(),
            dropped: false,
        }
    }

    /// Feeds one chunk and appends what it recognized to `out`, each report
    /// carrying the offset at which its sequence ended.
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<SniffedReport>) {
        let mut rest = bytes;

        while !rest.is_empty() {
            match self.state {
                State::Idle => {
                    let Some(position) = memchr::memchr(0x1b, rest) else {
                        return;
                    };
                    self.state = State::Escape;
                    rest = &rest[position + 1..];
                }
                State::Escape => {
                    let byte = rest[0];
                    rest = &rest[1..];
                    self.state = match byte {
                        b']' => {
                            self.payload.clear();
                            self.dropped = false;
                            State::Payload
                        }
                        0x1b => State::Escape,
                        _ => State::Idle,
                    };
                }
                State::Payload => {
                    let end = memchr::memchr2(0x07, 0x1b, rest);
                    let take = end.unwrap_or(rest.len());
                    self.push(&rest[..take]);
                    match end {
                        None => return,
                        Some(index) => {
                            let terminator = rest[index];
                            rest = &rest[index + 1..];
                            if terminator == 0x07 {
                                self.finish(out, bytes.len() - rest.len());
                            } else {
                                self.state = State::PayloadEscape;
                            }
                        }
                    }
                }
                State::PayloadEscape => {
                    let byte = rest[0];
                    rest = &rest[1..];
                    match byte {
                        b'\\' => self.finish(out, bytes.len() - rest.len()),
                        _ => {
                            self.push(&[0x1b, byte]);
                            self.state = State::Payload;
                        }
                    }
                }
            }
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        if self.payload.len() + bytes.len() > MAX_PAYLOAD {
            self.dropped = true;
            return;
        }
        self.payload.extend_from_slice(bytes);
    }

    fn finish(&mut self, out: &mut Vec<SniffedReport>, end: usize) {
        self.state = State::Idle;
        if self.dropped {
            self.payload.clear();
            return;
        }
        let payload = std::mem::take(&mut self.payload);
        if let Ok(text) = String::from_utf8(payload)
            && let Some(report) = parse(&text)
        {
            out.push(SniffedReport { end, report });
        }
    }
}

/// Turns the payload of one sequence into a report, when it is one we want.
fn parse(payload: &str) -> Option<OscReport> {
    let (code, rest) = match payload.split_once(';') {
        Some((code, rest)) => (code, rest),
        None => (payload, ""),
    };

    match code {
        "7" => Some(OscReport::WorkingDirectory(directory_of(rest)?)),
        "9" => match rest.strip_prefix("4;") {
            Some(rest) => progress(rest).map(OscReport::Progress),
            None => {
                let text = rest.trim();
                if text.is_empty() {
                    None
                } else {
                    Some(OscReport::Notification {
                        kind: NotificationKind::Text,
                        title: String::new(),
                        body: text.to_string(),
                    })
                }
            }
        },
        "777" => notification(rest),
        "133" => mark(rest).map(OscReport::Mark),
        _ => None,
    }
}

/// The state and the share of an OSC 9;4 sequence.
///
/// The share is missing on the states that have none, and a share outside a
/// hundred is brought back to it, because a bar cannot be fuller than full.
fn progress(rest: &str) -> Option<ProgressState> {
    let (state, share) = match rest.split_once(';') {
        Some((state, share)) => (state, share.trim().parse::<u32>().ok()?.min(100) as u8),
        None => (rest, 0),
    };

    match state.trim() {
        "0" => Some(ProgressState::Removed),
        "1" => Some(ProgressState::Set(share)),
        "2" => Some(ProgressState::Error(share)),
        "3" => Some(ProgressState::Indeterminate),
        "4" => Some(ProgressState::Paused(share)),
        _ => None,
    }
}

/// `file://host/path` and a plain path both name a directory.
fn directory_of(text: &str) -> Option<String> {
    let path = match text.strip_prefix("file://") {
        Some(rest) => &rest[rest.find('/')?..],
        None => text,
    };
    let path = percent_decode(path);
    if path.is_empty() { None } else { Some(path) }
}

/// Resolves the `%XX` escapes of a URI path.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let digits = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(digits, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

/// `777;notify;title;body`, the form every shell helper writes.
fn notification(rest: &str) -> Option<OscReport> {
    let mut parts = rest.splitn(3, ';');
    if parts.next()? != "notify" {
        return None;
    }
    let title = parts.next().unwrap_or_default().trim().to_string();
    let body = parts.next().unwrap_or_default().trim().to_string();
    if title.is_empty() && body.is_empty() {
        return None;
    }
    Some(OscReport::Notification {
        kind: NotificationKind::Titled,
        title,
        body,
    })
}

/// `133;A`, `133;B`, `133;C`, `133;D[;exit]`.
fn mark(rest: &str) -> Option<MarkKind> {
    let mut parts = rest.split(';');
    let kind = parts.next()?;
    match kind {
        "A" => Some(MarkKind::PromptStart),
        "B" => Some(MarkKind::CommandStart),
        "C" => Some(MarkKind::OutputStart),
        "D" => {
            let code = parts.next().and_then(|code| code.parse::<i32>().ok());
            Some(MarkKind::CommandEnd(code))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reports(chunks: &[&[u8]]) -> Vec<OscReport> {
        let mut sniffer = OscSniffer::new();
        let mut out = Vec::new();
        for chunk in chunks {
            sniffer.feed(chunk, &mut out);
        }
        out.into_iter().map(|one| one.report).collect()
    }

    #[test]
    fn a_progress_report_is_not_a_notification() {
        assert_eq!(
            reports(&[b"\x1b]9;4;1;50\x07"]),
            vec![OscReport::Progress(ProgressState::Set(50))]
        );
        assert_eq!(
            reports(&[b"\x1b]9;4;0\x07"]),
            vec![OscReport::Progress(ProgressState::Removed)]
        );
        assert_eq!(
            reports(&[b"\x1b]9;4;2;80\x07"]),
            vec![OscReport::Progress(ProgressState::Error(80))]
        );
        assert_eq!(
            reports(&[b"\x1b]9;4;3\x07"]),
            vec![OscReport::Progress(ProgressState::Indeterminate)]
        );
        assert_eq!(
            reports(&[b"\x1b]9;4;4;10\x07"]),
            vec![OscReport::Progress(ProgressState::Paused(10))]
        );
    }

    #[test]
    fn a_share_fuller_than_full_is_brought_back() {
        assert_eq!(
            reports(&[b"\x1b]9;4;1;250\x07"]),
            vec![OscReport::Progress(ProgressState::Set(100))]
        );
    }

    #[test]
    fn a_plain_notification_is_still_one() {
        assert_eq!(
            reports(&[b"\x1b]9;the build is done\x07"]),
            vec![OscReport::Notification {
                kind: NotificationKind::Text,
                title: String::new(),
                body: "the build is done".to_string(),
            }]
        );
        assert_eq!(reports(&[b"\x1b]9;4;9;1\x07"]), vec![]);
    }

    #[test]
    fn the_working_directory_is_read_from_a_uri() {
        assert_eq!(
            reports(&[b"\x1b]7;file://host/home/user/my%20work\x07"]),
            vec![OscReport::WorkingDirectory(
                "/home/user/my work".to_string()
            )]
        );
        assert_eq!(
            reports(&[b"\x1b]7;/srv/data\x1b\\"]),
            vec![OscReport::WorkingDirectory("/srv/data".to_string())]
        );
    }

    #[test]
    fn notifications_come_in_both_forms() {
        assert_eq!(
            reports(&[b"\x1b]9;build ready\x07"]),
            vec![OscReport::Notification {
                kind: NotificationKind::Text,
                title: String::new(),
                body: "build ready".to_string(),
            }]
        );
        assert_eq!(
            reports(&[b"\x1b]777;notify;Build;ready to flash\x07"]),
            vec![OscReport::Notification {
                kind: NotificationKind::Titled,
                title: "Build".to_string(),
                body: "ready to flash".to_string(),
            }]
        );
    }

    #[test]
    fn shell_marks_are_reported() {
        assert_eq!(
            reports(&[
                b"\x1b]133;A\x07",
                b"\x1b]133;B\x07",
                b"\x1b]133;C\x07",
                b"\x1b]133;D;3\x07",
            ]),
            vec![
                OscReport::Mark(MarkKind::PromptStart),
                OscReport::Mark(MarkKind::CommandStart),
                OscReport::Mark(MarkKind::OutputStart),
                OscReport::Mark(MarkKind::CommandEnd(Some(3))),
            ]
        );
    }

    #[test]
    fn a_sequence_survives_a_chunk_border() {
        assert_eq!(
            reports(&[b"\x1b", b"]13", b"3;A", b"\x07"]),
            vec![OscReport::Mark(MarkKind::PromptStart)]
        );
    }

    #[test]
    fn ordinary_output_and_other_sequences_are_ignored() {
        assert!(reports(&[b"plain text\r\n\x1b[31mred\x1b[0m"]).is_empty());
        assert!(reports(&[b"\x1b]0;title\x07\x1b]8;;https://example\x07"]).is_empty());
        assert!(reports(&[b"\x1b]133;Z\x07"]).is_empty());
    }

    #[test]
    fn an_endless_sequence_is_dropped_instead_of_kept() {
        let mut sniffer = OscSniffer::new();
        let mut out = Vec::new();
        sniffer.feed(b"\x1b]7;", &mut out);
        sniffer.feed(&vec![b'x'; MAX_PAYLOAD + 10], &mut out);
        sniffer.feed(b"\x07", &mut out);
        assert!(out.is_empty());
        assert!(sniffer.payload.is_empty());

        sniffer.feed(b"\x1b]133;A\x07", &mut out);
        assert_eq!(
            out,
            vec![SniffedReport {
                end: 8,
                report: OscReport::Mark(MarkKind::PromptStart),
            }]
        );
    }

    #[test]
    fn a_report_says_where_its_sequence_ended() {
        let mut sniffer = OscSniffer::new();
        let mut out = Vec::new();
        sniffer.feed(b"ls\x1b]133;C\x07output", &mut out);

        assert_eq!(
            out,
            vec![SniffedReport {
                end: 10,
                report: OscReport::Mark(MarkKind::OutputStart),
            }]
        );
    }

    #[test]
    fn an_offset_of_a_split_sequence_names_the_chunk_that_ended_it() {
        let mut sniffer = OscSniffer::new();
        let mut out = Vec::new();
        sniffer.feed(b"\x1b]133;", &mut out);
        assert!(out.is_empty());

        sniffer.feed(b"A\x1b\\rest", &mut out);
        assert_eq!(
            out,
            vec![SniffedReport {
                end: 3,
                report: OscReport::Mark(MarkKind::PromptStart),
            }]
        );
    }
}
