//! Events produced by the emulation while bytes are processed.

/// Something the application has to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    /// The program set a new window title.
    Title(String),
    /// The program asked to restore the default title.
    ResetTitle,
    /// The program rang the bell.
    Bell,
    /// The program asked to store text in the clipboard.
    ClipboardStore(String),
    /// The program asked for the clipboard. The answer is given with
    /// [`crate::Terminal::answer_clipboard`].
    ClipboardRequest,
    /// The program reported its working directory.
    WorkingDirectory(String),
    /// The program asked for a desktop notification.
    Notification {
        /// Sequence that asked for it.
        kind: crate::osc::NotificationKind,
        /// Heading of the notification, empty when none was given.
        title: String,
        /// Text of the notification.
        body: String,
    },
    /// The shell marked a point of its cycle.
    Mark(crate::osc::MarkKind),
    /// The shell marked a command, and this is the line that was typed.
    ///
    /// It is read out of the grid between the mark that opens the command and
    /// the mark that opens its output, because no sequence carries it.
    Command(String),
    /// The program reported how far along it is.
    Progress(crate::osc::ProgressState),
    /// The program asked to end the session.
    Exit,
}
