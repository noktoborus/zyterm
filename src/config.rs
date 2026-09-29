//! Settings of the application, stored as YAML in the platform configuration
//! directory.

use serde::{Deserialize, Serialize};
use zyt_serial::LineParams;

/// Seconds a hint waits when the wait is switched back on.
pub const DEFAULT_TOOLTIP_DELAY: f32 = 0.5;

/// Kibibytes of the buffer a source is read into, when it is switched back on.
pub const DEFAULT_READ_BUFFER: usize = 1024;

/// The smallest and the largest that buffer is offered at, in kibibytes.
///
/// The floor is a chunk of a read and a little over it, because a buffer under
/// one read would hold the source back on every single one of them; the cap is
/// what a window may be asked to keep of a device that never stops talking.
pub const READ_BUFFER_SIZES: std::ops::RangeInclusive<usize> = 64..=65_536;

/// Milliseconds between two readings of the modem lines, before the user
/// changes it.
pub const DEFAULT_LINES_INTERVAL: u32 = ms(zyt_serial::DEFAULT_LINES_INTERVAL);

/// The shortest and the longest wait the settings offer for it, in
/// milliseconds.
pub const LINES_INTERVAL_MS: std::ops::RangeInclusive<u32> =
    ms(*zyt_serial::LINES_INTERVAL_RANGE.start())..=ms(*zyt_serial::LINES_INTERVAL_RANGE.end());

/// A span of the port worker as the settings write it, in whole milliseconds.
const fn ms(span: std::time::Duration) -> u32 {
    span.as_millis() as u32
}

/// The ladder of waits a source is read at, before the user changes it.
///
/// A source saying a line now and then is taken as it speaks, because a wait
/// there is a wait the person sees. One pouring out text is taken in pieces
/// instead: every reading is a picture of the whole grid, and taking them
/// further apart makes each one a bigger piece of text rather than more of
/// them. The steps below are the three paces a console runs at — a person
/// typing, a program printing, a program flooding.
pub const DEFAULT_READ_STEPS: [ReadStep; 3] = [
    ReadStep {
        speed: 1,
        interval: None,
    },
    ReadStep {
        speed: 12,
        interval: Some(27),
    },
    ReadStep {
        speed: 40,
        interval: Some(60),
    },
];

/// What happens past the last step of the ladder, before the user changes it.
///
/// A source can always be faster than the last row, and a wait that stopped
/// growing there would be a window taking bigger and bigger pieces at the same
/// pace. The wait grows with the speed instead, and stops at a second of it:
/// past that the window would be a log nobody is watching.
pub const DEFAULT_READ_ABOVE: ReadAbove = ReadAbove {
    interval: Some(1000),
    linear: true,
};

/// The lowest and the highest speed a step is offered at, in kibibytes a
/// second.
pub const READ_STEP_SPEEDS: std::ops::RangeInclusive<u32> = 1..=65_536;

/// The shortest and the longest wait a step is offered at, in milliseconds.
pub const READ_STEP_INTERVALS: std::ops::RangeInclusive<u32> = 1..=1000;

/// The most steps the ladder is allowed to carry.
pub const READ_STEPS_MAX: usize = 8;

/// Name of the settings file.
/// Size the interface is drawn at when nobody has said otherwise, in points.
///
/// It is the size of the body text of the toolkit, which every other text of
/// the interface stands at a share of; it is also the size the terminal starts
/// at, which is a coincidence of the two defaults and not a rule — the two are
/// set apart because they are read at different distances and for different
/// lengths of time.
pub const DEFAULT_INTERFACE_FONT_SIZE: f32 = 13.0;

/// The smallest and the largest either font of the window is offered at.
pub const FONT_SIZES: std::ops::RangeInclusive<f32> = 8.0..=28.0;

/// Height of the status bar at which everything in it is drawn at its own size,
/// in points: the height of one control of the toolkit and the margin above and
/// below it.
pub const DEFAULT_STATUS_BAR_HEIGHT: f32 = 24.0;

/// The shortest and the tallest the status bar may be made.
pub const STATUS_BAR_HEIGHTS: std::ops::RangeInclusive<f32> = 18.0..=64.0;

/// How much everything in the status bar is drawn by, at this height.
///
/// The bar has no height of its own: it is as tall as what stands in it, so a
/// taller bar is asked for by drawing everything in it larger. The factor is
/// the height wanted over the height the toolkit draws at, which is why a bar
/// of 48 points carries buttons and letters of twice the size and not the same
/// buttons with empty room around them.
pub fn status_bar_scale(height: f32) -> f32 {
    (height / DEFAULT_STATUS_BAR_HEIGHT).clamp(0.5, 4.0)
}

pub const SETTINGS_FILE: &str = "settings.yaml";
/// Name of the key map file.
pub const KEYMAP_FILE: &str = "keymap.yaml";
/// Name of the file holding what the file dialog remembers.
pub const FILE_DIALOG_FILE: &str = "file-dialog.yaml";

/// How the interface picks its colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follow the desktop setting.
    System,
    /// Always dark.
    Dark,
    /// Always light.
    Light,
}

/// Language of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocaleSetting {
    /// Follow the environment.
    System,
    /// English.
    English,
    /// Russian.
    Russian,
}

impl LocaleSetting {
    /// Locale code used by the translation tables.
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Russian => "ru",
            Self::System => system_locale(),
        }
    }
}

fn system_locale() -> &'static str {
    let raw = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if raw.starts_with("ru") { "ru" } else { "en" }
}

/// What a program may do with the clipboard through OSC 52.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardSetting {
    /// Neither storing nor reading is honoured.
    Disabled,
    /// A program may store text and may read it.
    CopyPaste,
    /// A program may store text and may read back what was stored this way, and
    /// nothing else.
    ///
    /// Storing reaches the clipboard of the desktop as it does in every other
    /// mode, and a program that asks for the clipboard is answered from the
    /// buffer of this window instead: what the user copied somewhere else never
    /// leaves the desktop, and a program still gets its own text back — which is
    /// what a program that stores a line and reads it again is doing.
    ///
    /// It is the default, because it is the one setting that is not all or
    /// nothing: a program that stores a line and reads it back works, and the
    /// clipboard of the desktop is still never handed over.
    #[default]
    CopyLimitedPaste,
    /// A program may store text but never read it.
    CopyOnly,
}

/// Every clipboard setting, in the order a list offers them: from the one that
/// honours nothing to the one that honours everything.
pub const CLIPBOARD_SETTINGS: &[ClipboardSetting] = &[
    ClipboardSetting::Disabled,
    ClipboardSetting::CopyOnly,
    ClipboardSetting::CopyLimitedPaste,
    ClipboardSetting::CopyPaste,
];

impl ClipboardSetting {
    /// Translation key of the name shown in the settings.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Disabled => "settings.clipboard_disabled",
            Self::CopyPaste => "settings.clipboard_copy_paste",
            Self::CopyLimitedPaste => "settings.clipboard_copy_limited_paste",
            Self::CopyOnly => "settings.clipboard_copy_only",
        }
    }

    /// Translation key of what this setting does, which is what the plate
    /// beside the list says about the entry standing under the selection.
    ///
    /// Four names of three words each say which is which and not what any of
    /// them means, and the difference between them is the whole question: which
    /// clipboard is written and which one is read.
    pub fn hint_key(self) -> &'static str {
        match self {
            Self::Disabled => "settings.clipboard_disabled_hint",
            Self::CopyPaste => "settings.clipboard_copy_paste_hint",
            Self::CopyLimitedPaste => "settings.clipboard_copy_limited_paste_hint",
            Self::CopyOnly => "settings.clipboard_copy_only_hint",
        }
    }
}

/// What a program may do through the operating system commands, on each kind of
/// session.
///
/// The output of a program of this machine is trusted; the output of a device on
/// the other end of a line is not, so the two are set apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OscPair {
    /// What a program of a local console may do.
    #[serde(default = "OscSettings::trusted")]
    pub trusted: OscSettings,
    /// What a device, or a console that talks to one, may do.
    #[serde(default = "OscSettings::untrusted")]
    pub untrusted: OscSettings,
}

impl Default for OscPair {
    fn default() -> Self {
        Self {
            trusted: OscSettings::trusted(),
            untrusted: OscSettings::untrusted(),
        }
    }
}

/// What a program may do through the operating system commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OscSettings {
    /// OSC 52: what a program may do with the clipboard.
    #[serde(default)]
    pub clipboard: ClipboardSetting,
    /// OSC 9: a desktop notification with a text only.
    #[serde(default = "honoured")]
    pub notification_text: bool,
    /// OSC 777: a desktop notification with a heading and a text.
    #[serde(default = "honoured")]
    pub notification_titled: bool,
    /// OSC 0 and OSC 2: the window title a program sets.
    #[serde(default = "honoured")]
    pub title: bool,
    /// OSC 8: hyperlinks in the output.
    #[serde(default = "honoured")]
    pub links: bool,
    /// OSC 133: the marks a shell puts around its prompt and its commands.
    #[serde(default = "honoured")]
    pub marks: bool,
    /// OSC 9;4: how far along a program says it is.
    #[serde(default = "honoured")]
    pub progress: bool,
    /// OSC 4, 10, 11 and 12: the colors a program paints itself in.
    #[serde(default = "honoured")]
    pub palette: bool,
}

impl Default for OscSettings {
    fn default() -> Self {
        Self::trusted()
    }
}

impl OscSettings {
    /// What a guest whose output is trusted may do: everything the terminal
    /// knows, with the clipboard at the one setting that is not all or nothing.
    pub fn trusted() -> Self {
        Self {
            clipboard: ClipboardSetting::default(),
            notification_text: true,
            notification_titled: true,
            title: true,
            links: true,
            marks: true,
            progress: true,
            palette: true,
        }
    }

    /// What a guest whose output is not trusted may do: put text into the
    /// clipboard, and mark the cycle of its shell.
    ///
    /// Everything else reaches past the terminal — the desktop, the name of the
    /// window, the addresses the user can click, the colors the window wears —
    /// and a device on the other end of a line is not trusted with that.
    pub fn untrusted() -> Self {
        Self {
            clipboard: ClipboardSetting::CopyOnly,
            notification_text: false,
            notification_titled: false,
            title: false,
            links: false,
            marks: true,
            progress: true,
            palette: false,
        }
    }
}

/// Default of a status bar nobody resized: the height the toolkit draws at.
fn default_status_bar_height() -> f32 {
    DEFAULT_STATUS_BAR_HEIGHT
}

/// Default of every switch that is not the clipboard: the sequence is honoured.
fn honoured() -> bool {
    true
}

/// What the menu of a directory offers, and what it asks before it acts.
///
/// A directory is not a file: it is opened, moved, renamed and thrown away, and
/// nothing of it goes into a clipboard but its address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryMenu {
    /// Offer opening the directory with the file manager.
    #[serde(default = "yes")]
    pub open: bool,
    /// Offer choosing what opens it.
    #[serde(default = "yes")]
    pub open_with: bool,
    /// Offer renaming, which is a move under another name.
    #[serde(default = "yes")]
    pub rename: bool,
    /// Offer moving the directory into the directory of the session.
    #[serde(default = "yes")]
    pub move_here: bool,
    /// Offer copying the address of the directory.
    #[serde(default = "yes")]
    pub copy_link: bool,
    /// Offer copying the text the address was written as.
    #[serde(default = "yes")]
    pub copy_text: bool,
    /// Offer handing the directory to the trash of the desktop.
    #[serde(default = "yes")]
    pub trash: bool,
    /// Ask before the directory goes to the trash.
    #[serde(default)]
    pub trash_confirm: bool,
    /// Offer removing the directory, and everything under it, for good.
    #[serde(default = "yes")]
    pub delete: bool,
    /// Ask before the directory is removed for good.
    #[serde(default = "yes")]
    pub delete_confirm: bool,
}

impl Default for DirectoryMenu {
    fn default() -> Self {
        Self {
            open: true,
            open_with: true,
            rename: true,
            move_here: true,
            copy_link: true,
            copy_text: true,
            trash: true,
            trash_confirm: false,
            delete: true,
            delete_confirm: true,
        }
    }
}

/// What the menu of a file offers, and what it asks before it acts.
///
/// The limit of the clipboard is a size in bytes: a clipboard hands the whole
/// file to every program that asks for it, so what is carried has to stay
/// small enough to be carried twice over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMenu {
    /// Offer opening the file with the program of the desktop.
    #[serde(default = "yes")]
    pub open: bool,
    /// Offer choosing what opens it.
    #[serde(default = "yes")]
    pub open_with: bool,
    /// Offer renaming, which is a move under another name.
    #[serde(default = "yes")]
    pub rename: bool,
    /// Offer moving the file into the directory of the session.
    #[serde(default = "yes")]
    pub move_here: bool,
    /// Offer copying the address of the file.
    #[serde(default = "yes")]
    pub copy_link: bool,
    /// Offer copying the text the address was written as.
    #[serde(default = "yes")]
    pub copy_text: bool,
    /// Offer copying what the file holds.
    #[serde(default = "yes")]
    pub copy_contents: bool,
    /// Largest file whose contents are offered to the clipboard, in bytes.
    #[serde(default = "default_copy_limit")]
    pub copy_limit: u64,
    /// Offer handing the file to the trash of the desktop.
    #[serde(default = "yes")]
    pub trash: bool,
    /// Ask before the file goes to the trash.
    #[serde(default)]
    pub trash_confirm: bool,
    /// Offer removing the file for good.
    #[serde(default = "yes")]
    pub delete: bool,
    /// Ask before the file is removed for good.
    #[serde(default = "yes")]
    pub delete_confirm: bool,
}

impl Default for FileMenu {
    fn default() -> Self {
        Self {
            open: true,
            open_with: true,
            rename: true,
            move_here: true,
            copy_link: true,
            copy_text: true,
            copy_contents: true,
            copy_limit: default_copy_limit(),
            trash: true,
            trash_confirm: false,
            delete: true,
            delete_confirm: true,
        }
    }
}

/// Default of a switch that is on.
fn yes() -> bool {
    true
}

/// A mebibyte: what a clipboard carries without trouble.
///
/// The text goes to the clipboard whole and is handed over whole to every
/// program that asks for it, and a clipboard manager keeps a copy of its own,
/// so the file is in memory several times over. A mebibyte is far above any
/// configuration file, log excerpt or source file a terminal deals in, and far
/// below what makes a desktop stutter.
fn default_copy_limit() -> u64 {
    1024 * 1024
}

/// One palette per color mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemePair {
    /// Palette worn while the interface is dark.
    #[serde(default = "crate::themes::dark_name")]
    pub dark: String,
    /// Palette worn while the interface is light.
    #[serde(default = "crate::themes::light_name")]
    pub light: String,
}

impl Default for ThemePair {
    fn default() -> Self {
        Self {
            dark: crate::themes::dark_name(),
            light: crate::themes::light_name(),
        }
    }
}

impl ThemePair {
    /// Palette of one color mode.
    pub fn name(&self, dark: bool) -> &str {
        if dark { &self.dark } else { &self.light }
    }

    /// Palette of one color mode, to be replaced.
    pub fn name_mut(&mut self, dark: bool) -> &mut String {
        if dark {
            &mut self.dark
        } else {
            &mut self.light
        }
    }
}

/// The families the window is drawn with, one job each.
///
/// They are chosen apart because they are two different jobs: the terminal
/// wants a font whose every glyph is one cell wide, and the interface wants
/// one that reads well at small sizes. Nothing named here means the font the
/// toolkit ships with.
///
/// The terminal is a chain rather than one family, because one family is
/// rarely the whole of what a device prints: the letters come from the font
/// somebody chose for the grid, and a line drawing, a script of another
/// alphabet or a sign nobody planned for comes from whatever stands behind it.
/// The order is the order they are asked in, so the first one is the font of
/// the terminal and the rest are what it does not carry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontChoice {
    /// Families the terminal is drawn with, first one first.
    #[serde(default)]
    pub terminal: Vec<String>,
    /// Family the interface is drawn with.
    #[serde(default)]
    pub interface: Option<String>,
}

/// Size of the interface of a settings file that does not name one.
fn default_interface_font_size() -> f32 {
    DEFAULT_INTERFACE_FONT_SIZE
}

/// Commands one source keeps before the user changes the number.
fn default_command_history() -> usize {
    200
}

/// What a command picked out of the history does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryAction {
    /// Typed into the source and left standing, without the key that runs it.
    ///
    /// What stands in the shell is then a line to read once more and change
    /// before it runs.
    Insert,
    /// Typed in with the carriage return that runs it, for somebody who knows
    /// what they are picking.
    Run,
}

impl HistoryAction {
    /// Translation key of the name shown in the settings.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Insert => "settings.history_insert",
            Self::Run => "settings.history_run",
        }
    }

    /// Translation key of what it does, which is what the pointer uncovers:
    /// two names of one word say which is which and not what either of them
    /// does to the line.
    pub fn hint_key(self) -> &'static str {
        match self {
            Self::Insert => "settings.history_insert_hint",
            Self::Run => "settings.history_run_hint",
        }
    }
}

/// Both ways of choosing a command of the history, in the order the settings
/// offer them: the one that leaves it standing, then the one that runs it.
pub const HISTORY_ACTIONS: &[HistoryAction] = &[HistoryAction::Insert, HistoryAction::Run];

/// What each way of choosing a command of the history does with it.
///
/// There are two ways of choosing one and two things to do with it, so the two
/// are a pair and not a switch: somebody who picks a command to run it still
/// wants the other now and then, and reaching it should not be a walk into the
/// settings. Which key is which is the setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryKeys {
    /// What `Enter` does with the command under the selection.
    #[serde(default = "run")]
    pub enter: HistoryAction,
    /// What `Shift` and `Enter` do with it.
    #[serde(default = "insert")]
    pub shift_enter: HistoryAction,
}

impl Default for HistoryKeys {
    fn default() -> Self {
        Self {
            enter: run(),
            shift_enter: insert(),
        }
    }
}

impl HistoryKeys {
    /// What the keys that were held say to do with the command.
    pub fn action(self, shift: bool) -> HistoryAction {
        if shift { self.shift_enter } else { self.enter }
    }
}

/// What `Enter` does before the user says otherwise.
fn run() -> HistoryAction {
    HistoryAction::Run
}

/// What `Shift` and `Enter` do before the user says otherwise.
fn insert() -> HistoryAction {
    HistoryAction::Insert
}

/// Mebibytes the scrollback may take before the user says otherwise.
///
/// Sixteen is ten thousand lines in a narrow window and three thousand in a wide
/// one, which is the trade a budget makes: the window that costs the most is the
/// one that keeps the least.
fn default_scrollback_memory() -> usize {
    16
}

/// Fewest lines a scrollback keeps, whatever the budget says.
///
/// A window wide enough to spend the whole budget on a handful of lines still
/// keeps a page of them: a scrollback of nothing is a terminal that forgets the
/// line above the one being read.
pub const MIN_SCROLLBACK_LINES: usize = 500;
/// Most lines a scrollback keeps, whatever the budget says.
pub const MAX_SCROLLBACK_LINES: usize = 1_000_000;

/// How many lines a budget of mebibytes buys at this width.
///
/// A row costs its full width, so the lines are the budget divided by the width
/// of the window: a window that grows wider keeps fewer lines of the same memory
/// and drops the oldest of them, and one that grows narrower keeps more.
///
/// Sixteen mebibytes are some eight thousand lines at eighty columns and some
/// three thousand at two hundred, because the price of a line is the width of the
/// window and `zyt_term::GRID_CELL_BYTES` is the price of a cell.
pub fn scrollback_lines(memory: usize, columns: usize) -> usize {
    let row = columns.max(1) * zyt_term::GRID_CELL_BYTES;
    let lines = memory.saturating_mul(1024 * 1024) / row;
    lines.clamp(MIN_SCROLLBACK_LINES, MAX_SCROLLBACK_LINES)
}

/// Seconds a hint waits before it is shown, before the user says otherwise.
///
/// Waiting is what the toolkit does, and while it waits it asks for a frame
/// after every movement of the pointer, so a hint that waits for nothing costs
/// nothing either.
fn default_tooltip_delay() -> Option<f32> {
    Some(DEFAULT_TOOLTIP_DELAY)
}

/// Kibibytes of the buffer a source is read into, before the user says
/// otherwise.
///
/// A mebibyte is far more than a window takes in one reading and far less than
/// a window is hurt by keeping, so the buffer is never filled by a source the
/// window keeps up with and is filled at once by one it does not.
fn default_read_buffer() -> Option<usize> {
    Some(DEFAULT_READ_BUFFER)
}

/// How often the modem lines are read, before the user changes it.
fn default_lines_interval() -> u32 {
    DEFAULT_LINES_INTERVAL
}

/// The ladder of waits, before the user changes it.
fn default_read_steps() -> Vec<ReadStep> {
    DEFAULT_READ_STEPS.to_vec()
}

/// What happens past the last step, before the user changes it.
fn default_read_above() -> ReadAbove {
    DEFAULT_READ_ABOVE
}

/// One step of the ladder that says how long the bytes of a source wait before
/// they are taken.
///
/// A step holds up to its speed: the slowest one whose speed the source has not
/// passed is the one in force, and a source faster than every step runs at the
/// last of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadStep {
    /// Kibibytes a second this step holds up to.
    pub speed: u32,
    /// Milliseconds the bytes wait at this speed, or none for taking them
    /// whenever a frame asks.
    pub interval: Option<u32>,
}

/// What the ladder does past its last step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadAbove {
    /// The longest the bytes ever wait, in milliseconds, or none for taking
    /// them whenever a frame asks however fast the source runs.
    pub interval: Option<u32>,
    /// Whether the wait grows with the speed up to that, instead of jumping to
    /// it at the first byte past the last step.
    pub linear: bool,
}

impl ReadAbove {
    /// The wait at a speed past the last step, in milliseconds.
    ///
    /// Without the linear progression this is the wait itself, from the first
    /// byte past the last step. With it the wait grows in step with the speed —
    /// twice the speed of the last row is twice its wait — and stops here. The
    /// two meet at the speed of the last row, so the growth starts where the
    /// ladder ended and nothing jumps.
    ///
    /// A last row that waits for nothing, and a ladder with no rows at all,
    /// have nothing to grow from, so this wait stands from the first byte past
    /// the ladder.
    pub fn wait(&self, steps: &[ReadStep], speed: f32) -> Option<u32> {
        let longest = self.interval?;
        if !self.linear {
            return Some(longest);
        }

        let Some(last) = steps.iter().max_by_key(|step| step.speed) else {
            return Some(longest);
        };
        let Some(interval) = last.interval else {
            return Some(longest);
        };
        if last.speed == 0 {
            return Some(longest);
        }

        let grown = interval as f32 * speed.max(0.0) / last.speed as f32;
        Some((grown as u32).clamp(interval, longest))
    }
}

/// Where a speed falls on the ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPace {
    /// The step that holds this speed.
    Step(ReadStep),
    /// Past every step of the ladder.
    Above,
}

/// Where the given speed falls, in kibibytes a second.
///
/// The steps are read in the order they stand in, so a ladder nobody sorted is
/// answered by the slowest step that holds the speed rather than by the first
/// one written. A ladder with no steps at all is past every step from the
/// first byte.
pub fn read_pace(steps: &[ReadStep], speed: f32) -> ReadPace {
    let speed = speed.max(0.0);
    steps
        .iter()
        .filter(|step| speed <= step.speed as f32)
        .min_by_key(|step| step.speed)
        .copied()
        .map_or(ReadPace::Above, ReadPace::Step)
}

/// How long the bytes wait at the given speed, in milliseconds.
pub fn read_wait(steps: &[ReadStep], above: ReadAbove, speed: f32) -> Option<u32> {
    match read_pace(steps, speed) {
        ReadPace::Step(step) => step.interval,
        ReadPace::Above => above.wait(steps, speed),
    }
}

/// Baud rates offered before the user changes the list.
pub fn default_baud_rates() -> Vec<u32> {
    zyt_serial::COMMON_BAUD_RATES.to_vec()
}

/// Which source the terminal is connected to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceSetting {
    /// A serial port, addressed by its operating system path.
    Port {
        /// Path of the port.
        path: String,
    },
    /// A local console profile.
    Console {
        /// The console this is, whatever it is called.
        id: crate::consoles::ConsoleId,
    },
    /// Nothing is connected.
    #[default]
    None,
}

/// Everything the application remembers between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Interface language.
    pub locale: LocaleSetting,
    /// Interface colors.
    pub theme: ThemeMode,
    /// Terminal palettes, one per color mode.
    #[serde(default)]
    pub themes: ThemePair,
    /// What the menu of a file offers.
    #[serde(default)]
    pub file_menu: FileMenu,
    /// What the menu of a directory offers.
    #[serde(default)]
    pub directory_menu: DirectoryMenu,
    /// The bar that says where a selection would begin is drawn.
    ///
    /// It stands in the terminal wherever the last press landed, and it is off
    /// by default: the place is kept and `Shift` and a press still select from
    /// it whether it is drawn or not, so what the switch decides is whether
    /// there is a second mark in the grid beside the cursor.
    #[serde(default)]
    pub show_selection_anchor: bool,
    /// Status bar is visible.
    pub show_status_bar: bool,
    /// How tall the status bar stands, in points. Everything in it is drawn to
    /// match, so the number says the size of the bar and not the room around
    /// what is in it.
    #[serde(default = "default_status_bar_height")]
    pub status_bar_height: f32,
    /// The window of numbers is shown: what the program costs while it runs.
    #[serde(default)]
    pub show_debug_window: bool,
    /// Seconds the pointer stands on a control before its hint is shown, or
    /// none for a hint that is shown at once.
    #[serde(default = "default_tooltip_delay")]
    pub tooltip_delay: Option<f32>,
    /// Kibibytes of the buffer a source is read into, or none for a buffer with
    /// no limit.
    ///
    /// It is allocated once, at this size, and never grows: a full one stops
    /// the source being read. Nothing is thrown away then — what the source
    /// says waits where it was said, in the pipe of a console, whose program
    /// waits at its next write, and in the driver of a port, which tells the
    /// device to wait when the line has flow control. A window that cannot keep
    /// up is therefore a window that slows the other side down, and not one
    /// that grows until the memory of the machine is gone.
    #[serde(default = "default_read_buffer")]
    pub read_buffer: Option<usize>,
    /// How long the bytes of a source wait before they are taken and shown, by
    /// the speed the source is running at.
    ///
    /// It holds back the reading and nothing else: the interface is drawn as
    /// often as the toolkit draws it, and a transfer is never held back.
    #[serde(default = "default_read_steps")]
    pub read_steps: Vec<ReadStep>,
    /// How long the bytes wait past the last step of the ladder.
    #[serde(default = "default_read_above")]
    pub read_above: ReadAbove,
    /// Milliseconds between two readings of the modem lines of a port.
    ///
    /// Every reading is a call into the driver on the thread that reads the
    /// port, so the wait is what the state of `DTR`, `CTS` and the rest costs:
    /// a line watched closely is a line looked at instead of read.
    #[serde(default = "default_lines_interval")]
    pub lines_interval: u32,
    /// How the search bar reads a query, and what it marks.
    #[serde(default)]
    pub search: zyt_term::SearchOptions,
    /// Terminal font size in points.
    pub font_size: f32,
    /// Interface font size in points.
    ///
    /// It is the size of the body text; a heading and a hint follow it by the
    /// share they stand at in the sizes of the toolkit, so what is larger than
    /// the text stays larger.
    #[serde(default = "default_interface_font_size")]
    pub interface_font_size: f32,
    /// Families the window is drawn with, taken from the ones the system has.
    #[serde(default)]
    pub fonts: FontChoice,
    /// Mebibytes the scrollback of one window may take.
    ///
    /// It is a budget and not a count of lines, because a line does not cost the
    /// same everywhere: a row is kept at the full width of the window whatever
    /// little stands in it, so the same ten thousand lines are nineteen
    /// mebibytes in an eighty column window and ninety-four in a four hundred
    /// column one. How many lines the budget buys at the width of the moment is
    /// [`scrollback_lines`].
    #[serde(default = "default_scrollback_memory")]
    pub scrollback_memory: usize,
    /// Commands kept in the history of one source.
    #[serde(default = "default_command_history")]
    pub command_history: usize,
    /// What each way of choosing a command of the history does with it.
    ///
    /// It is typed into the source either way. What the setting decides is
    /// whether the carriage return that runs it follows, per key: `Enter` runs
    /// it and `Shift`+`Enter` leaves it standing, unless they are turned over.
    #[serde(default)]
    pub command_history_keys: HistoryKeys,
    /// Baud rates offered for a serial port.
    #[serde(default = "default_baud_rates")]
    pub baud_rates: Vec<u32>,
    /// What a program may do through the operating system commands, per kind of
    /// session.
    #[serde(default)]
    pub osc: OscPair,
    /// Source a window opens when nothing on the command line says otherwise.
    ///
    /// A port by its path or a console by its identity; a source that is not
    /// there any more leaves the window with the list of them, which is also
    /// what [`SourceSetting::None`] does. That variant is the whole of how
    /// nothing is said, so there is no second spelling of it to tell apart.
    #[serde(default)]
    pub default_source: SourceSetting,
    /// Source a window was connected to before the last disconnect, which is
    /// what it goes back to.
    #[serde(default)]
    pub last_source: SourceSetting,
    /// Line parameters a device that was never used starts with. Every device
    /// keeps its own from then on, in its own file.
    pub line: LineParams,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            locale: LocaleSetting::System,
            theme: ThemeMode::System,
            themes: ThemePair::default(),
            file_menu: FileMenu::default(),
            directory_menu: DirectoryMenu::default(),
            show_selection_anchor: false,
            show_status_bar: true,
            status_bar_height: DEFAULT_STATUS_BAR_HEIGHT,
            show_debug_window: false,
            tooltip_delay: default_tooltip_delay(),
            read_buffer: default_read_buffer(),
            read_steps: default_read_steps(),
            read_above: default_read_above(),
            lines_interval: default_lines_interval(),
            search: zyt_term::SearchOptions::default(),
            font_size: 13.0,
            interface_font_size: default_interface_font_size(),
            fonts: FontChoice::default(),
            scrollback_memory: default_scrollback_memory(),
            command_history: default_command_history(),
            command_history_keys: HistoryKeys::default(),
            baud_rates: default_baud_rates(),
            osc: OscPair::default(),
            default_source: SourceSetting::None,
            last_source: SourceSetting::None,
            line: LineParams::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a budget buys depends on the width, because a row costs its full
    /// width, and it is spent to within one row of what it says.
    #[test]
    fn the_lines_a_budget_buys_follow_the_width() {
        assert!(
            scrollback_lines(16, 80) > scrollback_lines(16, 200),
            "a narrower window keeps more lines of the same memory"
        );
        assert!(
            scrollback_lines(64, 200) > scrollback_lines(16, 200),
            "and a larger budget keeps more of them"
        );

        for (memory, columns) in [(16, 80), (16, 200), (16, 400), (64, 200)] {
            let lines = scrollback_lines(memory, columns);
            let spent = lines * columns * zyt_term::GRID_CELL_BYTES;
            let budget = memory * 1024 * 1024;

            assert!(spent <= budget, "{memory} MiB at {columns} columns");
            assert!(
                budget - spent < columns * zyt_term::GRID_CELL_BYTES,
                "the budget is spent to within one row"
            );
        }
    }

    /// Whatever the budget says, a window keeps a page of lines and never more
    /// than the cap: a scrollback of nothing is a terminal that forgets the line
    /// above the one being read.
    #[test]
    fn a_budget_is_held_between_a_page_and_the_cap() {
        assert_eq!(scrollback_lines(1, 4000), MIN_SCROLLBACK_LINES);
        assert_eq!(scrollback_lines(4096, 1), MAX_SCROLLBACK_LINES);
    }

    /// The clipboard each kind of guest starts with. A trusted one reads back
    /// what it stored itself, which is what a program that stores a line and
    /// reads it again needs, and neither of them is ever handed the clipboard
    /// of the desktop.
    #[test]
    fn a_trusted_guest_starts_with_limited_paste_and_a_device_with_none() {
        assert_eq!(
            OscSettings::trusted().clipboard,
            ClipboardSetting::CopyLimitedPaste
        );
        assert_eq!(
            OscSettings::untrusted().clipboard,
            ClipboardSetting::CopyOnly
        );
        assert_eq!(
            ClipboardSetting::default(),
            ClipboardSetting::CopyLimitedPaste,
            "a settings file that says nothing means this one"
        );
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use zyt_term::{SearchKind, SearchOptions};

    #[test]
    fn a_file_without_the_search_section_reads_the_plainest_search() {
        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("search"));

        let settings: Settings = serde_yaml_ng::from_value(value).expect("settings parse");

        assert_eq!(settings.search, SearchOptions::default());
        assert_eq!(settings.search.kind, SearchKind::Literal);
        assert!(!settings.search.case_sensitive);
        assert!(!settings.search.highlight_all);
    }

    #[test]
    fn a_file_without_the_ladder_takes_the_usual_one() {
        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("read_steps"));

        let settings: Settings = serde_yaml_ng::from_value(value).expect("settings parse");

        assert_eq!(settings.read_steps, DEFAULT_READ_STEPS.to_vec());
    }

    /// A ladder of the user's own comes back as it went in, the step that waits
    /// for nothing among them.
    #[test]
    fn a_ladder_survives_a_round_trip() {
        let settings = Settings {
            read_steps: vec![
                ReadStep {
                    speed: 2,
                    interval: None,
                },
                ReadStep {
                    speed: 100,
                    interval: Some(200),
                },
            ],
            ..Settings::default()
        };

        let text = serde_yaml_ng::to_string(&settings).expect("settings serialize");
        let read: Settings = serde_yaml_ng::from_str(&text).expect("settings parse");

        assert_eq!(read.read_steps, settings.read_steps);
    }

    /// The step in force is the slowest one the speed has not passed, wherever
    /// it stands in the ladder, and a speed past every step is past the ladder.
    #[test]
    fn the_ladder_answers_with_the_step_that_holds_the_speed() {
        let ladder = DEFAULT_READ_STEPS;
        let wait = |speed| read_wait(&ladder, DEFAULT_READ_ABOVE, speed);

        assert_eq!(wait(0.0), None, "silence waits for nothing");
        assert_eq!(wait(1.0), None, "and so does the first step");
        assert_eq!(wait(1.5), Some(27));
        assert_eq!(wait(12.0), Some(27));
        assert_eq!(wait(20.0), Some(60));
        assert_eq!(wait(40.0), Some(60));
        assert_eq!(
            read_pace(&ladder, 41.0),
            ReadPace::Above,
            "past the last step is past the ladder"
        );
    }

    /// Past the last step the wait grows in step with the speed and stops at
    /// the longest one: twice the speed of the last row is twice its wait, and
    /// the two meet at that row, so nothing jumps where the ladder ends.
    #[test]
    fn the_wait_grows_with_the_speed_past_the_ladder() {
        let ladder = DEFAULT_READ_STEPS;
        let wait = |speed| read_wait(&ladder, DEFAULT_READ_ABOVE, speed);

        assert_eq!(wait(40.0), Some(60), "the last step itself");
        assert_eq!(wait(40.5), Some(60), "and the first byte past it");
        assert_eq!(wait(80.0), Some(120), "twice the speed, twice the wait");
        assert_eq!(wait(400.0), Some(600));
        assert_eq!(wait(1024.0), Some(1000), "and it stops at the longest one");
        assert_eq!(wait(65536.0), Some(1000));
    }

    /// Without the linear progression the longest wait stands from the first
    /// byte past the last step.
    #[test]
    fn a_ladder_without_the_progression_steps_straight_up() {
        let above = ReadAbove {
            interval: Some(1000),
            linear: false,
        };
        let wait = |speed| read_wait(&DEFAULT_READ_STEPS, above, speed);

        assert_eq!(wait(40.0), Some(60), "the last step is still the last step");
        assert_eq!(wait(40.5), Some(1000));
        assert_eq!(wait(4096.0), Some(1000));
    }

    /// Past the ladder with no wait named at all, the bytes are taken as they
    /// come however fast the source runs.
    #[test]
    fn a_ladder_that_names_no_wait_above_waits_for_nothing() {
        let above = ReadAbove {
            interval: None,
            linear: true,
        };
        assert_eq!(read_wait(&DEFAULT_READ_STEPS, above, 4096.0), None);
    }

    /// A ladder written out of order is answered by the step that holds the
    /// speed and not by the first line of the file.
    #[test]
    fn a_ladder_out_of_order_is_read_by_speed() {
        let ladder = [
            ReadStep {
                speed: 64,
                interval: Some(90),
            },
            ReadStep {
                speed: 2,
                interval: None,
            },
        ];

        assert_eq!(read_wait(&ladder, DEFAULT_READ_ABOVE, 1.0), None);
        assert_eq!(read_wait(&ladder, DEFAULT_READ_ABOVE, 10.0), Some(90));
    }

    /// A ladder with no steps left is past every step from the first byte, so
    /// what stands above it is what answers.
    #[test]
    fn an_empty_ladder_is_answered_by_what_stands_above_it() {
        assert_eq!(read_pace(&[], 1000.0), ReadPace::Above);
        assert_eq!(
            read_wait(&[], DEFAULT_READ_ABOVE, 1000.0),
            Some(1000),
            "with nothing to grow from, the longest wait stands"
        );
    }

    #[test]
    fn a_file_without_the_hint_delay_takes_the_usual_one() {
        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("tooltip_delay"));

        let settings: Settings = serde_yaml_ng::from_value(value).expect("settings parse");

        assert_eq!(settings.tooltip_delay, Some(DEFAULT_TOOLTIP_DELAY));
    }

    #[test]
    fn a_hint_delay_that_was_switched_off_survives_a_round_trip() {
        let settings = Settings {
            tooltip_delay: None,
            ..Settings::default()
        };

        let text = serde_yaml_ng::to_string(&settings).expect("settings serialize");
        let read: Settings = serde_yaml_ng::from_str(&text).expect("settings parse");

        assert_eq!(read.tooltip_delay, None);
    }

    #[test]
    fn the_modes_of_the_search_survive_a_round_trip() {
        let settings = Settings {
            search: SearchOptions {
                kind: SearchKind::Regex,
                case_sensitive: true,
                highlight_all: true,
            },
            ..Settings::default()
        };

        let text = serde_yaml_ng::to_string(&settings).expect("settings serialize");
        let read: Settings = serde_yaml_ng::from_str(&text).expect("settings parse");

        assert_eq!(read.search, settings.search);
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn a_file_without_the_progress_switch_honours_it() {
        let mut value =
            serde_yaml_ng::to_value(OscSettings::trusted()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("progress"));

        let osc: OscSettings = serde_yaml_ng::from_value(value).expect("settings parse");

        assert!(osc.progress);
    }

    /// Every clipboard setting is named and explained, and no two of them share
    /// a key: the list shows the name and the plate beside it what the name
    /// means, so a key used twice would explain one setting with another.
    #[test]
    fn every_clipboard_setting_says_what_it_is_and_what_it_does() {
        let mut keys: Vec<&str> = Vec::new();
        for setting in CLIPBOARD_SETTINGS {
            keys.push(setting.label_key());
            keys.push(setting.hint_key());
        }

        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "a key stands for one setting");
    }

    /// A settings file that says nothing about the size of the interface is a
    /// window drawn at the size the toolkit draws at, and one that names a size
    /// keeps it: the two fonts of the window are set apart, so a terminal made
    /// larger does not make the settings larger with it.
    #[test]
    fn the_two_sizes_of_the_window_are_two_settings() {
        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("interface_font_size"));

        let settings: Settings = serde_yaml_ng::from_value(value).expect("settings parse");
        assert_eq!(settings.interface_font_size, DEFAULT_INTERFACE_FONT_SIZE);

        let named = Settings {
            interface_font_size: 20.0,
            ..Settings::default()
        };
        let text = serde_yaml_ng::to_string(&named).expect("settings serialize");
        let read: Settings = serde_yaml_ng::from_str(&text).expect("settings parse");

        assert_eq!(read.interface_font_size, 20.0);
        assert_eq!(read.font_size, named.font_size, "the terminal is its own");
    }

    /// The two ways of choosing a command of the history do the two things
    /// there are to do with one, and a file that says nothing about them gets
    /// both: `Enter` runs the command, `Shift` and `Enter` leave it standing.
    #[test]
    fn the_two_ways_of_choosing_a_command_do_the_two_things() {
        let file = serde_yaml_ng::to_string(&Settings::default()).expect("settings serialize");
        let settings: Settings = serde_yaml_ng::from_str(&file).expect("settings parse");
        let keys = settings.command_history_keys;

        assert_eq!(keys.action(false), HistoryAction::Run);
        assert_eq!(keys.action(true), HistoryAction::Insert);

        let turned = HistoryKeys {
            enter: HistoryAction::Insert,
            shift_enter: HistoryAction::Run,
        };
        assert_eq!(turned.action(false), HistoryAction::Insert);
        assert_eq!(turned.action(true), HistoryAction::Run);
    }

    /// A file written before the keys were a pair says nothing either of them
    /// is named in, so both stand at their defaults and nothing of the old
    /// spelling is read.
    #[test]
    fn a_file_that_names_neither_key_gets_both_defaults() {
        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("command_history_keys"));
        map.insert(
            serde_yaml_ng::Value::from("command_history_enter"),
            serde_yaml_ng::Value::from(true),
        );

        let read: Settings = serde_yaml_ng::from_value(value).expect("the file parses");
        assert_eq!(read.command_history_keys, HistoryKeys::default());
    }

    /// A file that says nothing about the buffer gets one, because a window
    /// that keeps whatever a device says until the memory is gone is not what
    /// anybody asked for; a file that says there is none is read as none.
    #[test]
    fn a_file_that_says_nothing_about_the_buffer_gets_one() {
        let file = serde_yaml_ng::to_string(&Settings::default()).expect("settings serialize");
        let settings: Settings = serde_yaml_ng::from_str(&file).expect("settings parse");
        assert_eq!(settings.read_buffer, Some(DEFAULT_READ_BUFFER));

        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("read_buffer"));
        let without: Settings = serde_yaml_ng::from_value(value).expect("the file parses");
        assert_eq!(without.read_buffer, Some(DEFAULT_READ_BUFFER));

        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.insert(
            serde_yaml_ng::Value::from("read_buffer"),
            serde_yaml_ng::Value::Null,
        );
        let none: Settings = serde_yaml_ng::from_value(value).expect("a file naming none parses");
        assert_eq!(none.read_buffer, None, "a buffer with no limit is none");
    }

    /// A file written before the line settings existed reads as the defaults of
    /// them, and the defaults are the ones the line wants: the driver buffers
    /// are emptied when a port opens and the modem lines are dropped when it
    /// closes.
    #[test]
    fn a_file_that_names_none_of_the_line_settings_gets_their_defaults() {
        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("lines_interval"));
        let line = map
            .get_mut(serde_yaml_ng::Value::from("line"))
            .and_then(serde_yaml_ng::Value::as_mapping_mut)
            .expect("the line is a mapping");
        line.remove(serde_yaml_ng::Value::from("flush_on_open"));
        line.remove(serde_yaml_ng::Value::from("hupcl"));

        let read: Settings = serde_yaml_ng::from_value(value).expect("the file parses");

        assert_eq!(read.lines_interval, DEFAULT_LINES_INTERVAL);
        assert!(read.line.flush_on_open);
        assert!(read.line.hupcl);
    }

    /// The wait between two readings of the modem lines is one the port worker
    /// will take: a setting the worker holds inside its own range would be a
    /// number the page shows and the line does not follow.
    #[test]
    fn the_wait_between_two_readings_is_one_the_worker_takes() {
        let settings = Settings::default();
        let range = zyt_serial::LINES_INTERVAL_RANGE;
        let wait = std::time::Duration::from_millis(u64::from(settings.lines_interval));

        assert!(range.contains(&wait), "{wait:?}");
        for offered in [*LINES_INTERVAL_MS.start(), *LINES_INTERVAL_MS.end()] {
            let wait = std::time::Duration::from_millis(u64::from(offered));

            assert!(range.contains(&wait), "{offered} ms");
        }
    }

    /// A file that does not name a height stands at the one the toolkit draws
    /// at, and one that names a height keeps it.
    #[test]
    fn a_file_without_a_status_bar_height_stands_at_the_usual_one() {
        let mut value = serde_yaml_ng::to_value(Settings::default()).expect("settings serialize");
        let map = value.as_mapping_mut().expect("settings are a mapping");
        map.remove(serde_yaml_ng::Value::from("status_bar_height"));

        let settings: Settings = serde_yaml_ng::from_value(value).expect("settings parse");
        assert_eq!(settings.status_bar_height, DEFAULT_STATUS_BAR_HEIGHT);
    }

    /// The height is a factor everything in the bar is drawn by, so twice the
    /// height is twice the size of what stands in it, and a height nobody could
    /// draw is brought back to one that can be.
    #[test]
    fn the_height_of_the_bar_is_what_stands_in_it_grows_by() {
        assert_eq!(status_bar_scale(DEFAULT_STATUS_BAR_HEIGHT), 1.0);
        assert_eq!(status_bar_scale(DEFAULT_STATUS_BAR_HEIGHT * 2.0), 2.0);
        assert_eq!(status_bar_scale(DEFAULT_STATUS_BAR_HEIGHT / 2.0), 0.5);

        assert_eq!(status_bar_scale(0.0), 0.5, "nothing is drawn at no height");
        assert_eq!(status_bar_scale(1000.0), 4.0, "nor at any height at all");
    }
}
