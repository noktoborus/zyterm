//! Application state and frame loop.

use crate::commands::{AppCommand, build_registry};
use crate::config::{
    FILE_DIALOG_FILE, HistoryAction, KEYMAP_FILE, SETTINGS_FILE, Settings, SourceSetting,
};
use crate::consoles::{Console, ConsoleId};
use crate::error::{AppError, Result};
use crate::session::{NoticeKind, Session};
use crate::sources::SourceKey;
use crate::theme::ThemeWatcher;
use crate::ui;
use rust_i18n::t;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zyt_config::ConfigStore;
use zyt_keymux::{
    CONTEXT_SETTINGS, CONTEXT_TERMINAL, CONTEXT_TRANSFER, CommandId, CommandRegistry, Dispatch,
    KeyDispatcher, KeyStroke, Keymap, default_keymap,
};
use zyt_serial::{PortId, PortInfo};
use zyt_term::SearchDirection;
use zyt_term_egui::{TerminalFont, TerminalTheme};
use zyt_xfer::{Direction, Target, TargetKind};

/// Context active while the file dialog is shown.
const CONTEXT_FILE_DIALOG: &str = "file_dialog";

/// How often the sources are listed again while their menu stands.
const SOURCES_INTERVAL: Duration = Duration::from_secs(1);

/// Which part of the interface has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The terminal.
    Terminal,
    /// The settings view.
    Settings,
    /// The file dialog.
    FileDialog,
    /// The status bar.
    StatusBar,
    /// The search bar.
    Search,
}

/// The shortest time between two readings of a source.
///
/// `None` takes what arrived whenever a frame asks. A wait holds back the
/// reading and nothing else: the window is drawn whenever the toolkit draws it,
/// and the bytes that arrive in between wait where the reading thread put them
/// and are taken in one piece.
fn read_interval(milliseconds: Option<u32>) -> Option<Duration> {
    let wait = milliseconds?;
    if wait == 0 {
        return None;
    }
    Some(Duration::from_millis(u64::from(wait)))
}

/// The wait the ladder asks for at the speed a source is running at.
fn read_interval_at(
    steps: &[crate::config::ReadStep],
    above: crate::config::ReadAbove,
    speed: f32,
) -> Option<Duration> {
    read_interval(crate::config::read_wait(steps, above, speed))
}

impl Focus {
    /// Who the keyboard really goes to. An open search bar holds it whole: what
    /// is typed into it must never reach the device behind it.
    fn with_search(self, search_open: bool) -> Self {
        if search_open && self == Self::Terminal {
            Self::Search
        } else {
            self
        }
    }
}

/// What the main area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainView {
    /// The terminal.
    Terminal,
    /// The settings.
    Settings,
    /// The file dialog of a transfer.
    FileDialog,
}

/// Something the user asked to delete, waiting for a confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingDelete {
    /// The transfer profile at this position.
    Profile {
        /// Position in the list of profiles.
        index: usize,
        /// Name shown in the question.
        name: String,
    },
}

/// How far the answer to a clipboard request of a program got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClipboardStage {
    /// No program is waiting.
    #[default]
    Idle,
    /// The toolkit was asked for the clipboard and its answer is awaited.
    Waiting,
}

/// Why the file dialog is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingPick {
    /// A file or a directory for a transfer.
    Transfer(Direction),
    /// Where a file of the session should land.
    Rename,
    /// A path to type into the session.
    TypePath,
    /// The directory a console starts in, for the console of that place in the
    /// list.
    ConsoleDirectory(usize),
}

/// A file operation waiting for an answer to its question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingFile {
    /// Hand the file to the trash of the desktop.
    Trash(std::path::PathBuf),
    /// Remove the file for good.
    Delete(std::path::PathBuf),
}

impl PendingFile {
    /// File the operation works on.
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Trash(path) | Self::Delete(path) => path,
        }
    }

    /// Translation key of the question, which a directory asks in its own
    /// words: what it takes with it is not one file.
    pub fn question_key(&self) -> &'static str {
        match (self, self.path().is_dir()) {
            (Self::Trash(_), false) => "confirm.trash_file",
            (Self::Trash(_), true) => "confirm.trash_directory",
            (Self::Delete(_), false) => "confirm.delete_file",
            (Self::Delete(_), true) => "confirm.delete_directory",
        }
    }
}

/// Why a window is asking about a source instead of showing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostReason {
    /// The program of the console ended, with the code it reported when the
    /// operating system had one to report. Anything but zero is a failure, and
    /// the message says which code it was.
    Ended {
        /// Code the program ended with.
        code: Option<i32>,
    },
    /// The source could not be opened at all.
    Unreachable,
}

/// A source the window cannot reach, and what became of it.
#[derive(Debug, Clone, PartialEq)]
pub struct LostSource {
    /// What the window was on, to open again.
    pub source: SourceSetting,
    /// What became of it.
    pub reason: LostReason,
    /// What to call it in the question.
    ///
    /// It is carried rather than read off the source, because a console is
    /// addressed by the name as it was written — `SSH {remote_host}` — and read
    /// by that name with the values it was opened with put in. The question is
    /// about the one that is gone, so what it says is what was running.
    pub shown: String,
}

impl LostSource {
    /// What went wrong, in one sentence, above the menu that asks what to do
    /// about it.
    ///
    /// A console that ended with a code of its own says the code: that is what
    /// tells a shell somebody left from one that failed to start at all.
    pub fn message(&self) -> String {
        let name = self.shown.as_str();
        match self.reason {
            LostReason::Ended { code: Some(code) } if code != 0 => {
                t!("lost.failed", name = name, code = code).to_string()
            }
            LostReason::Ended { .. } => t!("lost.ended", name = name).to_string(),
            LostReason::Unreachable => t!("lost.unreachable", name = name).to_string(),
        }
    }
}

/// The window that asks for the values a console needs before it is opened.
///
/// A console whose command line or directory asks for a value nothing answers
/// would start with a hole in it — `ssh @board` — so the window is put in front
/// of the connection instead of after it. What is typed is kept in a file of
/// its own, per source, and comes back the next time the same connection is
/// picked; it never reaches the settings of that source, which are what
/// somebody decided about the console rather than what they answered on the
/// way in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// Console to open once the values are given.
    pub console: ConsoleId,
    /// Key of that console, which is what the answers are kept under.
    pub key: SourceKey,
    /// One name and what has been typed for it, in the order they are asked.
    pub values: Vec<(String, String)>,
    /// True until the first field has been given the keyboard, which happens
    /// on the frame after the window first stands: a window that asks for
    /// something and leaves the keyboard elsewhere has to be clicked into
    /// before it can be answered.
    pub opening: bool,
}

/// Transient interface state.
pub struct UiState {
    /// What the main area shows.
    pub view: MainView,
    /// Part of the interface holding the keyboard. Written through
    /// [`App::give_keyboard`] and nowhere else.
    pub focus: Focus,
    /// Deletion waiting for a confirmation.
    pub pending_delete: Option<PendingDelete>,
    /// Whether the mouse reports a program asked for are answered.
    ///
    /// The mouse is handed over as soon as a program asks for it, and taken
    /// back by the sign in the status bar. It is handed over again by itself
    /// once no program is asking any more: the sign is gone then, and a window
    /// that keeps an answer nobody can see has nothing to say about it.
    pub mouse_reports: bool,
    /// Where the answer to a clipboard request of a program stands.
    pub clipboard_stage: ClipboardStage,
    /// When the toolkit was asked for the clipboard.
    pub clipboard_asked: Option<Instant>,
    /// True while a paste of the user is on its way from the toolkit.
    pub paste_pending: bool,
    /// The status bar should take the keyboard on the next frame.
    pub status_bar_focus_wanted: bool,
    /// Widget of the status bar that takes the keyboard first.
    pub status_bar_id: Option<egui::Id>,
    /// True while the plate of the times stands whether or not the pointer is
    /// on the button that opens it.
    pub data_plate_pinned: bool,
    /// True while the plate is held down under a pointer that would otherwise
    /// be opening it, until that pointer leaves.
    pub data_plate_hidden: bool,
    /// Where the plate of the times stood the last time it was drawn, so a
    /// click on it is told from a click beside it.
    pub data_plate_rect: egui::Rect,
    /// Where the button that opens the panel of what is running stood the last
    /// time it was drawn, for the same reason.
    pub tasks_button_rect: egui::Rect,
    /// True while the pointer rests on that button, which shows the panel
    /// without its buttons for as long as it does.
    pub tasks_hovered: bool,
    /// True while the pointer rests on one of the modem line indicators, which
    /// is what raises the plate of the signals over the terminal.
    pub signals_hovered: bool,
    /// True while that plate stands whether or not a pointer is on one of them.
    ///
    /// The right button on any of the letters turns it over, and the palette is
    /// the other way to it — for somebody who wants to watch the lines while
    /// working in the terminal rather than while holding a pointer still over
    /// three letters.
    pub signals_pinned: bool,
    /// True while the plate is held down under a pointer that would otherwise
    /// be raising it, until that pointer leaves.
    ///
    /// The press that unpins it lands on a letter the pointer is resting on,
    /// which is what would raise the plate again on the same frame. So it is
    /// held down until the pointer goes: pressed twice, the right button takes
    /// the plate down and keeps it down.
    pub signals_hidden: bool,
    /// The samples that plate last drew, kept so that copying them out of the
    /// worker allocates once and not once a frame.
    pub signal_samples: Vec<zyt_serial::LineSample>,
    /// The one value of the settings that is open for writing, if any.
    pub editing: crate::ui::widgets::Editing,
    /// The window asking for the values a console needs, while it stands.
    pub ask: Option<Ask>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: MainView::Terminal,
            focus: Focus::Terminal,
            pending_delete: None,
            mouse_reports: true,
            clipboard_stage: ClipboardStage::Idle,
            clipboard_asked: None,
            paste_pending: false,
            status_bar_focus_wanted: false,
            status_bar_id: None,
            data_plate_pinned: false,
            data_plate_hidden: false,
            data_plate_rect: egui::Rect::NOTHING,
            tasks_button_rect: egui::Rect::NOTHING,
            tasks_hovered: false,
            signals_hovered: false,
            signals_pinned: false,
            signals_hidden: false,
            signal_samples: Vec::new(),
            editing: crate::ui::widgets::Editing::default(),
            ask: None,
        }
    }
}

/// Which half of the settings is shown.
///
/// The settings fall into two: what this window is like, and what one device is
/// like. The second half is a different answer per device — a speed, the values
/// a transfer profile asks for, the profiles offered at all — and a page that
/// mixed the two would say nothing about which of them a control belonged to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    /// What this window is like, whatever it is connected to.
    #[default]
    General,
    /// What one port or console is like.
    Connection,
}

/// The application.
pub struct App {
    /// Stored settings.
    pub settings: Settings,
    /// Access to the configuration files.
    pub store: ConfigStore,
    /// The selection of the platform, which the middle button pastes.
    pub primary: crate::primary::Primary,
    /// Link the pointer is on, shown in the corner of the terminal.
    pub hovered_link: Option<zyt_term_egui::LinkTarget>,
    /// Link the user opened a menu for.
    pub link_menu: Option<zyt_term_egui::LinkTarget>,
    /// Slot this copy of the application holds.
    /// Copies of the application running when this one started.
    /// Connection and terminal.
    pub session: Session,
    /// Color mode.
    pub theme: ThemeWatcher,
    /// Terminal palette of the current color mode.
    pub terminal_theme: TerminalTheme,
    /// Palettes to choose from.
    pub themes: crate::themes::ThemeCatalog,
    /// Consoles, one file each.
    pub consoles: Vec<Console>,
    /// True once a frame has been drawn, which is when the window is there to
    /// be looked at.
    shown: bool,
    /// True while the menu of the sources is waiting for the window to be
    /// there, which is when the ports may be looked up.
    source_menu_wanted: bool,
    /// True while the menu standing is the menu of the sources.
    source_menu: bool,
    /// When the sources were last listed, while that menu stands.
    sources_listed: Option<Instant>,
    /// True while the session had a source, to notice the moment it loses one.
    had_source: bool,
    /// What is remembered about every serial port, one file each.
    pub ports_memory: std::collections::BTreeMap<PortId, crate::sources::PortMemory>,
    /// Terminal font.
    pub font: TerminalFont,
    /// Shapes of the grid, kept between frames.
    pub terminal_cache: zyt_term_egui::TerminalCache,
    /// Transfer profiles, in a file of their own.
    pub profiles: Vec<zyt_xfer::TransferProfile>,
    /// Query and modes of the search bar.
    pub search: crate::search::SearchState,
    /// Key bindings.
    pub dispatcher: KeyDispatcher,
    /// Commands of the palette.
    pub registry: CommandRegistry,
    /// Menu of plates: the context menu, the link menu, the palette and the
    /// transfer control all open this one widget.
    pub menu: plate_menu::PlateMenu,
    /// File operations, each on a thread of its own.
    pub tasks: zyt_files::TaskRunner,
    /// The transfer programs running beside the line, each with its own file
    /// of output.
    pub jobs: zyt_xfer::JobRunner,
    /// True while the panel of running tasks stands on its own, which is what
    /// a press on its button leaves behind.
    pub show_tasks: bool,
    /// Which half of the settings is shown.
    pub settings_tab: SettingsTab,
    /// Source the connection settings are showing, or nothing for the one this
    /// window is connected to.
    pub settings_source: Option<SourceKey>,
    /// Ports found in the system that the process may open.
    pub ports: Vec<PortInfo>,
    /// Ports that exist but cannot be opened by this process.
    pub hidden_ports: usize,
    /// Transient interface state.
    pub ui: UiState,
    /// File dialog shown in place of the terminal.
    pub file_dialog: egui_file_dialog::FileDialog,
    /// What the open file dialog is picking for.
    pub pending_pick: Option<PendingPick>,
    /// Source the window cannot reach, waiting for the question about it to be
    /// answered.
    pub lost: Option<LostSource>,
    /// File dropped onto the window, waiting for the menu to say what to do
    /// with it.
    pub dropped: Option<std::path::PathBuf>,
    /// Reads whose bytes go into the session instead of the clipboard.
    typing_reads: std::collections::BTreeSet<zyt_files::TaskId>,
    /// What the process costs, read from the operating system for the window of
    /// numbers and for nobody else.
    pub meter: crate::metrics::Meter,
    /// What a program of a session last put into the clipboard through OSC 52.
    ///
    /// It is the clipboard of this window: the limited setting answers a reading
    /// program from here and never from the desktop, so a program reads back what
    /// a program of this window stored and nothing a user copied elsewhere. It is
    /// written whatever the setting says, because it costs a string and the
    /// setting may be turned over while a session runs.
    pub stored_clipboard: String,
    /// File the menu picked, waiting for the dialog to say where it lands.
    pub renaming: Option<std::path::PathBuf>,
    /// File operation waiting for its question to be answered.
    pub pending_file: Option<PendingFile>,
    /// View the file dialog returns to.
    return_view: MainView,
    last_focus: Focus,
    /// Whether a transfer was running when the contexts of the key bindings
    /// were last settled.
    last_transferring: bool,
    /// Whether a menu stood open when they were last settled.
    last_menu_open: bool,
    last_notice: Option<String>,
    shown_title: Option<String>,
    /// True while the window is flagged as wanting attention.
    attention_asked: bool,
    /// When this pass began.
    ///
    /// What the meter is told is the time between the start of the work of a
    /// pass and the end of it, which is what this program costs a frame. What
    /// the toolkit spends after that — cutting the shapes, handing them to the
    /// chip — is not in it, and neither is the wait between two passes: a
    /// window that draws when there is something to draw waits as long as
    /// nothing happens, and the length of that wait says nothing about what a
    /// frame costs.
    frame_started: Option<Instant>,
    fonts_dirty: bool,
    /// True on the pass that handed new font definitions over, until the pass
    /// that has them throws the kept picture away.
    fonts_settling: bool,
    /// True from the pass a palette changed in until the pass after it has
    /// built the picture of the grid again.
    theme_settling: bool,
    /// The share last handed to the desktop, so it is handed over on a change
    /// and not on every frame.
    shown_progress: Option<zyt_term::ProgressState>,
    /// Every family the system has, read the first time they are offered.
    font_families: Option<Vec<crate::fonts::Family>>,
    notify: Arc<dyn Fn() + Send + Sync>,
    /// How long bytes that arrived may wait before they wake the window, in
    /// microseconds; zero is at once. The worker threads read it, so the limit
    /// can be changed while they run.
    wake_delay: Arc<std::sync::atomic::AtomicU64>,
}

impl App {
    /// Builds the application state for a freshly created window.
    pub fn new(
        context: &egui::Context,
        store: ConfigStore,
        settings: Settings,
        console: Option<ConsoleId>,
    ) -> Result<Self> {
        let keymap = load_keymap(&store)?;
        let theme = ThemeWatcher::new(settings.theme, context.clone());
        context.set_visuals(theme.visuals());
        release_quit_key(context);
        crate::fonts::apply(
            context,
            &settings.fonts.terminal,
            settings.fonts.interface.as_deref(),
        );

        let wake_delay = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let notify: Arc<dyn Fn() + Send + Sync> = {
            let context = context.clone();
            let delay = wake_delay.clone();
            Arc::new(move || {
                let micros = delay.load(std::sync::atomic::Ordering::Relaxed);
                if micros == 0 {
                    context.request_repaint();
                } else {
                    context.request_repaint_after(Duration::from_micros(micros));
                }
            })
        };

        let mut dispatcher = KeyDispatcher::new(keymap);
        dispatcher.set_contexts(&[CONTEXT_TERMINAL]);

        let catalog = crate::themes::ThemeCatalog::load(&store);
        let mut app = Self {
            session: Session::new(crate::config::scrollback_lines(
                settings.scrollback_memory,
                zyt_term::TerminalConfig::default().columns,
            ))?,
            terminal_theme: theme.terminal_theme(),
            themes: catalog,
            consoles: crate::consoles::load_all(&store),
            profiles: crate::profiles::load(&store),
            ports_memory: crate::sources::load_ports(&store),
            font: TerminalFont {
                size: settings.font_size,
                family: crate::fonts::terminal_family(),
                ..TerminalFont::default()
            },
            terminal_cache: zyt_term_egui::TerminalCache::new(),
            theme,
            dispatcher,
            registry: build_registry(),
            menu: plate_menu::PlateMenu::new(),
            tasks: zyt_files::TaskRunner::new(),
            jobs: zyt_xfer::JobRunner::new(),
            show_tasks: false,
            settings_tab: SettingsTab::General,
            settings_source: None,
            ports: Vec::new(),
            shown: false,
            source_menu_wanted: false,
            source_menu: false,
            sources_listed: None,
            had_source: false,
            hidden_ports: 0,
            ui: UiState::default(),
            search: crate::search::SearchState {
                options: settings.search,
                ..crate::search::SearchState::default()
            },
            file_dialog: egui_file_dialog::FileDialog::new()
                .as_modal(false)
                .resizable(false)
                .movable(false),
            pending_pick: None,
            typing_reads: std::collections::BTreeSet::new(),
            meter: crate::metrics::Meter::new(),
            stored_clipboard: String::new(),
            lost: None,
            dropped: None,
            renaming: None,
            pending_file: None,
            return_view: MainView::Terminal,
            last_focus: Focus::Terminal,
            last_transferring: false,
            last_menu_open: false,
            last_notice: None,
            shown_title: None,
            attention_asked: false,
            frame_started: None,
            fonts_dirty: false,
            fonts_settling: false,
            theme_settling: false,
            shown_progress: None,
            font_families: None,
            notify,
            wake_delay,
            primary: crate::primary::Primary::default(),
            hovered_link: None,
            link_menu: None,
            store,
            settings,
        };

        app.apply_osc_settings();
        app.apply_read_interval();
        app.apply_read_buffer();
        app.apply_lines_interval();
        app.apply_tooltip_delay(context);
        app.apply_terminal_theme();
        app.apply_interface_size(context);
        app.report_broken_themes();
        app.open_first_source(console);
        app.apply_title(context);
        Ok(app)
    }

    /// Names the window after what it is connected to, or after the program.
    fn apply_title(&self, context: &egui::Context) {
        let title = match &self.shown_title {
            Some(reported) if !reported.trim().is_empty() => reported.clone(),
            _ => crate::WINDOW_TITLE.to_string(),
        };
        context.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    /// Marks the window as wanting attention while a bell rings behind it.
    ///
    /// A bell is what a program rings when it wants the person back, and a
    /// window that already has them needs nothing: one rung while the window
    /// has the keyboard is answered by the window itself being read, so it
    /// asks for nothing. Rung behind another window, it flags the window in
    /// the way the desktop flags one — a taskbar entry that flashes, an icon
    /// that bounces — and the flag stays until the window is looked at.
    ///
    /// Taking the flag down when the window comes forward is done here and not
    /// left to the platform: the toolkit says the request is reset on focus,
    /// but whether it really is belongs to the window manager, and a window
    /// still flashing while it is being read is worse than one that never
    /// flashed.
    fn handle_bell(&mut self, context: &egui::Context) {
        let rang = std::mem::take(&mut self.session.bell);
        let focused = context
            .input(|input| input.viewport().focused)
            .unwrap_or(true);

        if focused {
            if self.attention_asked {
                self.attention_asked = false;
                context.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                    egui::UserAttentionType::Reset,
                ));
            }
            return;
        }

        if rang && !self.attention_asked {
            self.attention_asked = true;
            context.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
    }

    /// Makes this source the one a window opens, or leaves none.
    ///
    /// Choosing the source that already is the one takes it back off, so a
    /// window started with nothing to say comes up with the list of them.
    pub fn set_default_source(&mut self, source: SourceSetting) {
        let wanted = match self.settings.default_source == source {
            true => SourceSetting::None,
            false => source,
        };
        if self.settings.default_source == wanted {
            return;
        }

        self.settings.default_source = wanted;
        self.save_settings();
    }

    /// Opens what a window starts with.
    ///
    /// The command line names a console, and what it names is what opens;
    /// without it the source of the settings opens — a port as readily as a
    /// console — and without that the menu of the sources is what the window
    /// comes up with. A source that is not there any more is said in the
    /// terminal and asks the same menu, because a window that opens nothing has
    /// to say why.
    fn open_first_source(&mut self, asked: Option<ConsoleId>) {
        let wanted = match asked {
            Some(id) => SourceSetting::Console { id },
            None => self.settings.default_source.clone(),
        };

        match wanted {
            SourceSetting::None => self.source_menu_wanted = true,
            source => self.connect_source(source),
        }
    }

    /// Connects to the port with the given path, using the parameters this
    /// device was used with before.
    pub fn connect_port(&mut self, path: &str) -> Result<()> {
        let outcome = self.open_port(path);
        self.ask_when_lost(
            &outcome,
            SourceSetting::Port {
                path: path.to_string(),
            },
        );
        outcome
    }

    fn open_port(&mut self, path: &str) -> Result<()> {
        self.last_notice = None;
        let target = self
            .ports
            .iter()
            .find(|port| port.path == path)
            .map(PortInfo::id)
            .unwrap_or_else(|| PortId::Path(path.to_string()));
        let params = self.line_for(&target);

        self.session.connect_serial(
            target.clone(),
            path.to_string(),
            params,
            Some(self.notify.clone()),
        )?;

        self.settings.last_source = SourceSetting::Port {
            path: path.to_string(),
        };
        let memory = self.port_memory_mut(&target);
        memory.line = params;
        memory.source.last_connected = Some(jiff::Timestamp::now());
        self.save_memory(&SourceKey::Port(target));
        self.save_settings();
        self.apply_terminal_theme();
        self.apply_osc_settings();
        self.show_view(MainView::Terminal);
        Ok(())
    }

    /// What is remembered about one source, if it was used before.
    pub fn memory(&self, key: &SourceKey) -> Option<&crate::sources::SourceMemory> {
        match key {
            SourceKey::Console(id) => self
                .consoles
                .iter()
                .find(|console| console.id == *id)
                .map(|console| &console.memory),
            SourceKey::Port(id) => self.ports_memory.get(id).map(|memory| &memory.source),
        }
    }

    /// What is remembered about one source, created on first use.
    ///
    /// A console keeps it in its own file, a device in a file of its own, so
    /// nothing of one source ever reaches another. A console this window has
    /// never heard of has nothing to write to: an identity is not something a
    /// console can be made out of, so there is nothing here and nothing is
    /// invented.
    pub fn memory_mut(&mut self, key: &SourceKey) -> Option<&mut crate::sources::SourceMemory> {
        match key {
            SourceKey::Console(id) => self
                .consoles
                .iter_mut()
                .find(|console| console.id == *id)
                .map(|console| &mut console.memory),
            SourceKey::Port(id) => {
                Some(&mut self.ports_memory.entry(id.clone()).or_default().source)
            }
        }
    }

    /// Writes what is remembered about one source to the file it lives in.
    pub fn save_memory(&mut self, key: &SourceKey) {
        match key {
            SourceKey::Console(id) => {
                if let Some(index) = self.consoles.iter().position(|console| console.id == *id) {
                    self.save_console(index);
                }
            }
            SourceKey::Port(id) => {
                let Some(memory) = self.ports_memory.get(id).cloned() else {
                    return;
                };
                let outcome = crate::sources::save_port(&self.store, id, &memory);
                self.report(outcome);
            }
        }
    }

    /// What is remembered about one device, created on first use.
    pub fn port_memory_mut(&mut self, id: &PortId) -> &mut crate::sources::PortMemory {
        self.ports_memory.entry(id.clone()).or_default()
    }

    /// Line parameters remembered for one device, or the ones a device that was
    /// never used starts with.
    pub fn line_for(&self, id: &PortId) -> zyt_serial::LineParams {
        self.ports_memory
            .get(id)
            .map(|memory| memory.line)
            .unwrap_or(self.settings.line)
    }

    /// Key of the memory entry of the current source.
    pub fn memory_key(&self) -> Option<SourceKey> {
        if let Some(id) = self.session.console_id() {
            return Some(SourceKey::Console(id));
        }
        self.active_port_key()
    }

    /// The values the current source keeps for the lines of a profile.
    ///
    /// A source that keeps none answers with nothing, and a profile that asks
    /// for a name then refuses to start rather than running with a hole in its
    /// command line.
    pub fn source_variables(&self) -> std::collections::BTreeMap<String, String> {
        self.memory_key()
            .and_then(|key| self.memory(&key))
            .map(crate::sources::SourceMemory::variable_map)
            .unwrap_or_default()
    }

    /// Source the connection settings act on: the one that was picked, or the
    /// one this window is connected to while none was.
    ///
    /// A pick that names nothing any more — a console that was thrown away, a
    /// device whose file is gone — is no pick at all: the page would otherwise
    /// write a console back into being under the name of one that was removed.
    pub fn settings_source_key(&self) -> Option<SourceKey> {
        self.settings_source
            .clone()
            .filter(|key| self.known_sources().iter().any(|(known, _)| known == key))
            .or_else(|| self.memory_key())
    }

    /// Every source there is to configure: the consoles, and the port this
    /// window is on while it is on one.
    ///
    /// The consoles are there because each of them is a file somebody wrote and
    /// meant: it is configured whether or not anything is running it. A port is
    /// not — it is a device the system found — so the one that is worth a page
    /// is the one on the line, where what a setting does can be seen the moment
    /// it is made. The devices a file is kept for are not offered either: their
    /// files are read when they come back, and a list of every board ever
    /// plugged in is a list nobody walks.
    pub fn known_sources(&self) -> Vec<(SourceKey, String)> {
        let mut sources: Vec<(SourceKey, String)> = self
            .consoles
            .iter()
            .map(|console| (console.key(), console.display_name()))
            .collect();

        if let Some(key) = self.active_port_key() {
            let label = self
                .session
                .port_path()
                .and_then(|path| self.ports.iter().find(|port| port.path == path))
                .map(PortInfo::label)
                .unwrap_or_else(|| key.to_string());
            sources.push((key, label));
        }
        sources
    }

    /// Name of one source as the list of them writes it.
    pub fn source_label(&self, key: &SourceKey) -> String {
        if let Some(id) = key.console() {
            return self
                .console(&id)
                .map(crate::consoles::Console::display_name)
                .unwrap_or_else(|| key.to_string());
        }
        self.known_sources()
            .into_iter()
            .find(|(known, _)| known == key)
            .map(|(_, label)| label)
            .unwrap_or_else(|| key.to_string())
    }

    /// The speeds the menu of the line offers for one device: its own list, or
    /// the shared one while it has none.
    pub fn baud_rates_of(&self, id: Option<&PortId>) -> Vec<u32> {
        id.and_then(|id| self.ports_memory.get(id))
            .map(|memory| memory.baud_rates.clone())
            .filter(|rates| !rates.is_empty())
            .unwrap_or_else(|| self.settings.baud_rates.clone())
    }

    /// The transfer profiles offered for the current source.
    pub fn offered_profiles(&self) -> Vec<&zyt_xfer::TransferProfile> {
        self.offered_profiles_of(self.memory_key().as_ref())
    }

    /// The transfer profiles one source is willing to offer.
    ///
    /// A source that names none is willing to offer them all, which is what
    /// every source meant before it could name any. What it is willing to offer
    /// and what it can actually run are two questions; this is the first, and
    /// it is what the settings ask.
    pub fn ticked_profiles_of(&self, key: Option<&SourceKey>) -> Vec<&zyt_xfer::TransferProfile> {
        let named = key
            .and_then(|key| self.memory(key))
            .map(|memory| memory.profiles.clone())
            .unwrap_or_default();

        if named.is_empty() {
            return self.profiles.iter().collect();
        }
        self.profiles
            .iter()
            .filter(|profile| named.contains(&profile.name))
            .collect()
    }

    /// The transfer profiles one source can actually run.
    ///
    /// A profile that asks this source for a value it has not got is left out:
    /// it would refuse at the moment it was started, and a list of what can be
    /// started now is the one thing a menu of them is opened for. The settings
    /// show it all the same, with a warning saying what it wants, because that
    /// is where the want is answered.
    pub fn offered_profiles_of(&self, key: Option<&SourceKey>) -> Vec<&zyt_xfer::TransferProfile> {
        self.ticked_profiles_of(key)
            .into_iter()
            .filter(|profile| self.missing_variables_of(profile, key).is_empty())
            .collect()
    }

    /// The values a profile asks one source for and that source has not got.
    pub fn missing_variables_of(
        &self,
        profile: &zyt_xfer::TransferProfile,
        key: Option<&SourceKey>,
    ) -> Vec<String> {
        let answered = key
            .and_then(|key| self.memory(key))
            .map(crate::sources::SourceMemory::variable_map)
            .unwrap_or_default();

        profile
            .variables()
            .into_iter()
            .filter(|name| !answered.contains_key(name))
            .collect()
    }

    /// Transfer profile of the current source.
    pub fn active_profile(&self) -> Option<&zyt_xfer::TransferProfile> {
        let name = self
            .memory_key()
            .and_then(|key| self.memory(&key)?.transfer_profile.clone());
        crate::profiles::find(&self.offered_profiles(), name.as_deref())
    }

    /// Turns the break condition of the transmission line over.
    ///
    /// A break is a state and not a character: it stands until it is taken
    /// back, which is why this is a switch and not a key that is sent. A session
    /// that is on no line answers that it is not connected, the way every other
    /// control of the line does.
    pub fn toggle_break(&mut self) {
        let held = !self.session.held_break;
        let outcome = self.session.set_break(held);
        self.report(outcome);
    }

    /// Tells the session how often the modem lines of a port are read.
    pub fn apply_lines_interval(&mut self) {
        let interval = std::time::Duration::from_millis(u64::from(self.settings.lines_interval));
        let outcome = self.session.set_lines_interval(interval);
        self.report(outcome);
    }

    /// Applies line parameters and remembers them for the current device.
    ///
    /// A line is a thing a device has, so what is written down is written down
    /// for a device: the session is asked which one it is on, and a session
    /// that is on a console is a session with no line to remember.
    pub fn set_line_params(&mut self, params: zyt_serial::LineParams) {
        if params == self.session.params {
            return;
        }
        let outcome = self.session.set_params(params);
        self.report(outcome);
        let Some(id) = self.active_port_id() else {
            return;
        };
        self.port_memory_mut(&id).line = params;
        self.save_memory(&SourceKey::Port(id));
    }

    /// Remembers a transfer profile for the current source.
    pub fn set_transfer_profile(&mut self, name: String) {
        let Some(key) = self.memory_key() else {
            return;
        };
        if let Some(memory) = self.memory_mut(&key) {
            memory.transfer_profile = Some(name);
        }
        self.save_memory(&key);
    }

    /// Identity of the device the session is connected to.
    pub fn active_port_id(&self) -> Option<PortId> {
        let path = self.session.port_path()?;
        Some(
            self.ports
                .iter()
                .find(|port| port.path == path)
                .map(PortInfo::id)
                .unwrap_or_else(|| PortId::Path(path.to_string())),
        )
    }

    /// Key of the device the session is connected to.
    pub fn active_port_key(&self) -> Option<SourceKey> {
        self.active_port_id().map(SourceKey::Port)
    }

    /// Ends the connection, remembering what was connected so the next click
    /// can restore it.
    ///
    /// The list of the sources is asked for, the way it is asked for whenever a
    /// session ends: a window is let go of one source to be pointed at another,
    /// and this is also where a question about a source the window cannot reach
    /// leads when it is answered that way.
    pub fn disconnect(&mut self) {
        self.session.disconnect();
        self.save_settings();
        self.apply_terminal_theme();
        self.source_menu_wanted = true;
    }

    /// Whether the mouse reports a program asked for are answered right now.
    ///
    /// The sign in the status bar says which it is and pressing it turns it
    /// over; holding `ctrl+shift` turns it over as well, for as long as it is
    /// held, whichever way it stands. One answer serves the widget and the
    /// sign, so what the pointer does and what the bar says are never two
    /// different things.
    pub fn mouse_reports(&self, context: &egui::Context) -> bool {
        let held = context.input(|input| input.modifiers.ctrl && input.modifiers.shift);
        self.ui.mouse_reports != held
    }

    /// Puts the question about a source the window cannot reach in front of it.
    ///
    /// A window that cannot open what it was opened for has three ways out, and
    /// which one it is is not for the application to choose: the device may be
    /// back in a moment, another one may be the one that was meant, and a
    /// window with nothing to show may simply be done. The failure itself is
    /// still printed where every other failure is printed, so the question says
    /// what to do and not what went wrong.
    fn ask_when_lost(&mut self, outcome: &Result<()>, source: SourceSetting) {
        if outcome.is_ok() {
            return;
        }
        let shown = self.source_shown(&source);
        self.ask_about_lost(LostSource {
            source,
            reason: LostReason::Unreachable,
            shown,
        });
    }

    /// What a source is called where it is read.
    ///
    /// A port is its path. A console is its name with the values it answers put
    /// in — the ones typed into the window that asks for them included, because
    /// those are what it is opened with. A name nothing answers is left
    /// standing as it was written, which says what was wanted where an empty
    /// space would say nothing.
    pub fn source_shown(&self, source: &SourceSetting) -> String {
        match source {
            SourceSetting::Port { path } => path.clone(),
            SourceSetting::Console { id } => self.console_shown(*id),
            SourceSetting::None => String::new(),
        }
    }

    /// What one console is called where it is read.
    ///
    /// The console that is running says this itself — `Session::console_shown`,
    /// kept from the moment it started — so this is for the ones that are not:
    /// a console that failed to open, one that is about to be started again.
    pub fn console_shown(&self, id: ConsoleId) -> String {
        let Some(console) = self.console(&id) else {
            return id.to_string();
        };

        let mut memory = console.memory.clone();
        crate::answers::fill(
            &mut memory,
            &crate::answers::load(&self.store, &console.key()),
        );
        crate::sources::expand(&console.name, &memory.variable_map())
    }

    /// Opens the menu that asks what to do about a source the window cannot
    /// reach, with what went wrong above its entries.
    ///
    /// The three ways out are plates like any other menu of this window, so the
    /// keys that walk a menu walk this one as well. A menu that is closed
    /// without choosing anything leads to the picker, which is what the file
    /// dialog and the drop menu do with a question nobody answered: a question
    /// waved away must not be the one that closes the window.
    ///
    /// A click beside it is no answer, which is the one menu of this window that
    /// holds on that way: a console that died under a pointer doing something
    /// else would otherwise take its question with it before it was read. `Esc`
    /// and the entries are the ways out.
    fn ask_about_lost(&mut self, lost: LostSource) {
        let items = crate::ui::menu::lost_items();
        let message = lost.message();
        self.lost = Some(lost);
        match self.menu.open(items) {
            Ok(()) => {
                self.menu.notice(message);
                self.menu.beside(plate_menu::Beside::Ignored);
                self.show_view(MainView::Terminal);
            }
            Err(error) => {
                log::debug!("lost source menu: {error}");
                self.lost = None;
                self.disconnect();
            }
        }
    }

    /// Opens a source the menu of the sources named.
    ///
    /// A console that asks for a value nothing answers is asked about before it
    /// is opened, not after: what it would run without that value is a command
    /// line with a hole in it.
    pub fn open_source(&mut self, source: SourceSetting) {
        let SourceSetting::Console { id } = &source else {
            self.connect_source(source);
            return;
        };

        match crate::ui::ask::wanted(self, *id) {
            Some(ask) => self.ui.ask = Some(ask),
            None => self.connect_source(source),
        }
    }

    /// Opens what a setting names, which is how every source is opened.
    pub fn connect_source(&mut self, source: SourceSetting) {
        let outcome = match source {
            SourceSetting::Port { path } => self.connect_port(&path),
            SourceSetting::Console { id } => self.connect_console(id),
            SourceSetting::None => {
                self.open_source_menu();
                Ok(())
            }
        };
        self.report(outcome);
    }

    /// Source that was connected before the last disconnect.
    pub fn previous_source(&self) -> Option<SourceSetting> {
        match self.settings.last_source {
            SourceSetting::None => None,
            ref source => Some(source.clone()),
        }
    }

    /// Connects to the source that was used before the last disconnect.
    pub fn reconnect_previous(&mut self) {
        self.connect_source(self.settings.last_source.clone());
    }

    /// True while what the session prints may be acted on.
    ///
    /// A console says so itself, which is a judgement of the user. A device on
    /// the other end of a line never does: nothing about a line says who is
    /// answering on it.
    pub fn session_is_trusted(&self) -> bool {
        self.session
            .console_id()
            .and_then(|id| self.console(&id))
            .is_some_and(|console| console.trusted)
    }

    /// What the current session may do through the operating system commands:
    /// the trusted side of the settings for a session whose output is trusted,
    /// the other side for one whose output is not.
    pub fn osc(&self) -> &crate::config::OscSettings {
        if self.session_is_trusted() {
            &self.settings.osc.trusted
        } else {
            &self.settings.osc.untrusted
        }
    }

    /// The console this identity names, and nothing when this window has none
    /// of it.
    ///
    /// An identity that names no console names no console: it used to fall back
    /// to the first one in the list, which opened a console nobody asked for
    /// whenever a name had gone out of date.
    pub fn console(&self, id: &ConsoleId) -> Option<&Console> {
        self.consoles.iter().find(|console| console.id == *id)
    }

    /// Writes one console.
    pub fn save_console(&mut self, index: usize) {
        let Some(console) = self.consoles.get(index).cloned() else {
            return;
        };
        let outcome = crate::consoles::save(&self.store, &console);
        self.report(outcome);
    }

    /// Turns the judgement about the console of this session over, and writes it
    /// down where the settings keep it.
    ///
    /// The sign in the status bar is that judgement and the way to change it:
    /// what a console carries is known while it runs — an `ssh` was opened, a log
    /// is being followed — and walking into the settings to say so is walking away
    /// from the thing being judged. A session on a line has nothing to turn: a
    /// device is never trusted, because nothing about a line says who answers on
    /// it.
    ///
    /// The clipboard access and the marks of the session follow at once, the way
    /// they do when the settings change.
    pub fn toggle_session_trusted(&mut self) {
        let Some(id) = self.session.console_id() else {
            return;
        };
        let Some(index) = self.consoles.iter().position(|console| console.id == id) else {
            return;
        };

        let trusted = !self.consoles[index].trusted;
        self.consoles[index].trusted = trusted;
        log::info!(
            "the console {id} is {}trusted now",
            if trusted { "" } else { "not " }
        );
        self.save_console(index);
        self.apply_osc_settings();
    }

    /// Throws one console away, file and all.
    pub fn remove_console(&mut self, index: usize) {
        if index >= self.consoles.len() || self.consoles.len() == 1 {
            return;
        }
        let console = self.consoles.remove(index);
        let outcome = crate::consoles::remove(&self.store, &console);
        self.report(outcome);
    }

    /// Reads the palettes again, for a theme directory that changed while the
    /// window was open.
    pub fn reload_themes(&mut self) {
        self.themes = crate::themes::ThemeCatalog::load(&self.store);
        self.apply_terminal_theme();
        self.report_broken_themes();
    }

    /// Names the theme files that could not be read, once, in the terminal.
    fn report_broken_themes(&mut self) {
        let broken = self.themes.failed().join(", ");
        if broken.is_empty() {
            return;
        }
        self.notice(t!("themes.broken", names = broken).to_string());
    }

    /// Takes the palettes named in the settings.
    ///
    /// The color mode is the interface mode, so the terminal follows the
    /// desktop and wears the palette chosen for that mode.
    ///
    /// The picture of the grid is thrown away with it. It is built once and
    /// kept between frames, and every letter in it was cut in the colors of the
    /// palette it was built against: a page that says nothing new would go on
    /// being painted in the colors of the palette before this one.
    pub fn apply_terminal_theme(&mut self) {
        let dark = self.theme.is_dark();
        let name = self.settings.themes.name(dark).to_string();
        self.terminal_theme = self.themes.colors(&name, dark);
        self.terminal_cache.forget();
        self.theme_settling = true;
    }

    /// Builds the picture of the grid again on the pass after a palette
    /// changed.
    ///
    /// The pass the palette changed in builds it once already, and that one is
    /// built in the middle of everything else that pass does: a panel that was
    /// open closes, a menu goes away, a hint is laid out. Anything of that which
    /// puts a glyph in the atlas that was not there before moves every glyph of
    /// the picture just built, because a mesh of text names them as a share of
    /// the size of that atlas.
    ///
    /// So the picture is built once more, on a pass of its own, after all of it
    /// has happened and against the atlas as it stands then. `watch_the_atlas`
    /// is what asks for that pass, at the end of the one that changed the
    /// palette.
    fn settle_theme(&mut self) {
        if !self.theme_settling {
            return;
        }
        self.theme_settling = false;
        self.terminal_cache.forget();
    }

    /// Starts the console this identity names.
    pub fn connect_console(&mut self, id: ConsoleId) -> Result<()> {
        let outcome = self.open_console(id);
        self.ask_when_lost(&outcome, SourceSetting::Console { id });
        outcome
    }

    fn open_console(&mut self, id: ConsoleId) -> Result<()> {
        let Some(mut console) = self.console(&id).cloned() else {
            return Err(AppError::NoConsole);
        };
        let key = console.key();
        let answered = crate::answers::load(&self.store, &key);
        crate::answers::fill(&mut console.memory, &answered);
        if let Some(directory) = console.working_directory() {
            enter_directory(&directory);
        }
        self.session
            .connect_console(&console, Some(self.notify.clone()))?;

        self.settings.last_source = SourceSetting::Console { id };
        if let Some(memory) = self.memory_mut(&key) {
            memory.last_connected = Some(jiff::Timestamp::now());
        }
        self.save_memory(&key);
        self.save_settings();
        self.apply_terminal_theme();
        self.apply_osc_settings();
        self.show_view(MainView::Terminal);
        Ok(())
    }

    /// Writes the transfer profiles, which live in a file of their own.
    pub fn save_profiles(&mut self) {
        let outcome = crate::profiles::save(&self.store, &self.profiles);
        self.report(outcome);
    }

    /// Writes the settings file.
    pub fn save_settings(&mut self) {
        let outcome = self
            .store
            .save(SETTINGS_FILE, &self.settings)
            .map_err(AppError::from);
        self.report(outcome);
    }

    /// Starts another window.
    ///
    /// The new window opens the console this one is on, named on its command
    /// line; a window on a port or on the picker names nothing, and the new one
    /// opens whatever the settings say it should.
    ///
    /// The program is the one this window was started as, argv[0], run the way
    /// a shell would run it: the name is looked up the way it was looked up the
    /// first time, so a copy started from a build directory starts that build
    /// and one started from the path starts the one on the path.
    ///
    /// It starts in the directory this window is in, which a console keeps up
    /// to date through OSC 7, so the shell of the new window opens where the
    /// shell of this one stands. A console that names a directory of its own
    /// still wins over it.
    pub fn start_new_window(&mut self) {
        let Some(program) = std::env::args_os().next() else {
            log::warn!("the program was started with no name, so there is none to start again");
            return;
        };

        let mut command = std::process::Command::new(program);
        if let Some(id) = self.session.console_id() {
            command.arg("--console").arg(id.to_string());
        }

        if let Err(error) = command.spawn() {
            log::warn!("another window could not be started: {error}");
        }
    }

    /// Prints the outcome of an action into the terminal.
    ///
    /// Failures are blocks in the terminal instead of a line in the status bar,
    /// so they stay readable and stay in the history next to the output they
    /// belong to.
    pub fn report(&mut self, outcome: Result<()>) {
        let Err(error) = outcome else {
            return;
        };
        log::warn!("{error}: {error:?}");

        let mut lines = vec![t!(error.message_key()).to_string()];
        let detail = Detail(&error).to_string();
        if !detail.is_empty() {
            lines.push(detail);
        }

        let text = lines.join(" | ");
        if self.last_notice.as_deref() == Some(text.as_str()) {
            return;
        }
        self.last_notice = Some(text);
        self.session.print_block(NoticeKind::Error, &lines);
    }

    /// Prints a plain message into the terminal.
    pub fn notice(&mut self, text: String) {
        self.last_notice = None;
        self.session.print_block(NoticeKind::Info, &[text]);
    }

    /// Runs one command.
    pub fn run_command(&mut self, command: AppCommand, context: &egui::Context) {
        match command {
            AppCommand::PaletteOpen => self.open_palette(),
            AppCommand::SettingsOpen => {
                if self.ui.view == MainView::Settings {
                    self.show_view(MainView::Terminal);
                } else {
                    self.read_sources_again();
                    self.show_view(MainView::Settings);
                }
            }
            AppCommand::SettingsClose => self.show_view(MainView::Terminal),
            AppCommand::ToggleStatusBar => {
                self.settings.show_status_bar = !self.settings.show_status_bar;
                self.save_settings();
            }
            AppCommand::PortReopen => {
                let outcome = self.session.reopen();
                self.report(outcome);
            }
            AppCommand::PortOpenPrevious => self.reconnect_previous(),
            AppCommand::PortToggleBreak => self.toggle_break(),
            AppCommand::ToggleSignals => self.ui.signals_pinned = !self.ui.signals_pinned,
            AppCommand::PortChoose => self.open_source_menu(),
            AppCommand::PortDisconnect => {
                // It is the way to the list of sources and not only the way
                // off a line: pressed with nothing connected, what it is asked
                // for is that list.
                match self.session.has_source() {
                    true => self.disconnect(),
                    false => self.open_source_menu(),
                }
            }
            AppCommand::Quit => context.send_viewport_cmd(egui::ViewportCommand::Close),
            AppCommand::NewWindow => self.start_new_window(),
            AppCommand::FocusStatusBar => {
                self.give_keyboard(Focus::StatusBar);
                self.ui.status_bar_focus_wanted = true;
            }
            AppCommand::FocusTerminal => {
                let focus = match self.ui.view {
                    MainView::Settings => Focus::Settings,
                    MainView::FileDialog => Focus::FileDialog,
                    MainView::Terminal => Focus::Terminal,
                };
                self.give_keyboard(focus);
                self.ui.status_bar_focus_wanted = false;
                if let Some(id) = self.ui.status_bar_id {
                    context.memory_mut(|memory| memory.surrender_focus(id));
                }
            }
            AppCommand::Copy => self.copy_selection(context),
            AppCommand::Paste => self.paste_clipboard(context),
            AppCommand::Clear => self.session.clear_screen(),
            AppCommand::ScrollPageUp => {
                let rows = self.session.terminal.size().1 as i32;
                self.session.terminal.scroll(rows);
            }
            AppCommand::ScrollPageDown => {
                let rows = self.session.terminal.size().1 as i32;
                self.session.terminal.scroll(-rows);
            }
            AppCommand::SendFile => self.transfer(Direction::Send),
            AppCommand::ReceiveFile => self.transfer(Direction::Receive),
            AppCommand::CancelTransfer => {
                let outcome = self.session.cancel_transfer();
                self.report(outcome);
            }
            AppCommand::SearchOpen => {
                if self.ui.view == MainView::Terminal {
                    self.search.open(&mut self.session.terminal);
                    self.give_keyboard(Focus::Search);
                }
            }
            AppCommand::SearchNext => self
                .search
                .advance(&mut self.session.terminal, SearchDirection::Down),
            AppCommand::SearchPrevious => self
                .search
                .advance(&mut self.session.terminal, SearchDirection::Up),
            AppCommand::SearchClose => {
                self.close_search();
                context.memory_mut(|memory| {
                    memory.surrender_focus(crate::ui::search::field_id());
                });
            }
            AppCommand::HistoryOpen => self.open_history_menu(),
            AppCommand::ToggleMouseReports => self.ui.mouse_reports = !self.ui.mouse_reports,
        }
    }

    /// Hands the keyboard to a part of the interface.
    pub fn give_keyboard(&mut self, to: Focus) {
        self.ui.focus = to.with_search(self.search.open);
    }

    /// Takes the search bar down and hands the keyboard back.
    pub fn close_search(&mut self) {
        if !self.search.open {
            return;
        }
        self.search.close(&mut self.session.terminal);
        if self.ui.focus == Focus::Search {
            self.give_keyboard(Focus::Terminal);
        }
    }

    /// Switches the main area and moves the keyboard focus with it.
    pub fn show_view(&mut self, view: MainView) {
        self.close_search();
        let previous = self.ui.view;
        self.ui.view = view;
        self.give_keyboard(match view {
            MainView::Terminal => Focus::Terminal,
            MainView::Settings => Focus::Settings,
            MainView::FileDialog => Focus::FileDialog,
        });
        if view == MainView::FileDialog {
            if previous != MainView::FileDialog {
                self.return_view = previous;
            }
        } else {
            self.pending_pick = None;
        }
    }

    /// Opens the palette: every command as a plate, searched by typing.
    pub fn open_palette(&mut self) {
        let items = crate::ui::menu::palette_items(self);
        if let Err(error) = self.menu.open(items) {
            log::debug!("palette: {error}");
        }
    }

    /// Writes down the commands the shell of the source marked.
    ///
    /// The session only holds them until somebody asks, because where a
    /// command is kept is a question about configuration files, which it knows
    /// nothing of.
    fn write_down_commands(&mut self) {
        let commands = self.session.take_commands();
        if commands.is_empty() {
            return;
        }
        let Some(key) = self.memory_key() else {
            return;
        };
        let limit = self.settings.command_history;
        let directory = self
            .working_directory()
            .map(|directory| directory.display().to_string())
            .unwrap_or_default();
        for command in commands {
            crate::history::remember(&self.store, &key, &command, &directory, limit);
        }
    }

    /// The commands of the current source as the file stands right now.
    pub fn command_history(&self) -> Vec<crate::history::Entry> {
        match self.memory_key() {
            Some(key) => crate::history::load(&self.store, &key),
            None => Vec::new(),
        }
    }

    /// True while the current source has commands to offer.
    ///
    /// The status bar asks this every frame to decide whether to draw the
    /// button at all, so it is the size of the file and not its contents.
    pub fn has_command_history(&self) -> bool {
        self.memory_key()
            .is_some_and(|key| crate::history::has_any(&self.store, &key))
    }

    /// Opens the menu of the session, which the name of the source leads to.
    ///
    /// It stands where a control that ended the connection stood, so the thing
    /// that cannot be taken back is one entry among several instead of the first
    /// thing a pointer reaches.
    pub fn open_session_menu(&mut self) {
        let items = crate::ui::menu::session_items(self);
        if let Err(error) = self.menu.open(items) {
            log::debug!("session menu: {error}");
        }
    }

    /// Opens the commands the shell of this source marked.
    ///
    /// A history nothing has written to yet opens nothing and says nothing: a
    /// console that reports no commands is the ordinary case, and a message
    /// about it would stand in the output of the console it is about.
    pub fn open_history_menu(&mut self) {
        let items = crate::ui::menu::history_items(self);
        self.menu.shift(plate_menu::Shift::Always);
        if let Err(error) = self.menu.open(items) {
            log::debug!("command history: {error}");
        }
    }

    /// Types one command of the history into the source, and runs it when that
    /// is what the key it was chosen by asks for.
    ///
    /// The command is typed in either way. What follows it is the carriage
    /// return that runs it, or nothing and then what stands in the shell is a
    /// line to read once more and change before it runs. A device expects that
    /// key as a carriage return and not as a line feed, the way every other
    /// line this application types into one is closed.
    ///
    /// Nothing is written down either way — an entry says when it last ran, and
    /// a command that was typed back has not run yet. The shell says when it
    /// does, and that is what moves it to the top.
    pub fn run_from_history(&mut self, command: String, action: HistoryAction) {
        self.session.write(command.as_bytes());
        if action == HistoryAction::Run {
            self.session.write(b"\r");
        }
    }

    /// Opens the menu of the ways a search query can be read.
    pub fn open_search_kind_menu(&mut self) {
        let items = crate::ui::menu::search_kind_items(self);
        let current = crate::ui::menu::current_search_kind(self);
        if let Err(error) = self.menu.open_at(items, &current) {
            log::debug!("search kinds: {error}");
        }
    }

    /// Takes the way a search query is read, remembers it and searches again.
    pub fn set_search_kind(&mut self, kind: zyt_term::SearchKind) {
        self.search.options.kind = kind;
        self.settings.search = self.search.options;
        self.save_settings();
        self.search.apply(&mut self.session.terminal);
    }

    /// Every command the palette offers, in the order the registry returns.
    pub fn palette_hits(&mut self) -> Vec<zyt_keymux::Hit> {
        let contexts: Vec<zyt_keymux::Context> = vec![
            zyt_keymux::Context::new(CONTEXT_TERMINAL),
            zyt_keymux::Context::new(CONTEXT_SETTINGS),
        ];
        let keymap = self.dispatcher.keymap().clone();
        self.registry
            .search("", &contexts, &keymap)
            .into_iter()
            .filter(|hit| AppCommand::from_id(&hit.command.id).is_some())
            .collect()
    }

    fn copy_selection(&mut self, context: &egui::Context) {
        if let Some(text) = self.session.terminal.selected_text() {
            context.copy_text(text);
        }
    }

    /// Pastes the clipboard into the terminal.
    ///
    /// The toolkit owns the clipboard, so the request goes to it and comes back
    /// as a paste event, the same path a key press takes.
    /// Pastes what the middle button pastes on this platform: the selection of
    /// the platform, the selection of this terminal, or the clipboard.
    pub fn paste_selection_or_clipboard(&mut self, context: &egui::Context) {
        let selection = self.primary.text().or_else(|| {
            self.session
                .terminal
                .selected_text()
                .filter(|text| !text.is_empty())
        });

        match selection {
            Some(text) => {
                let modes = self.session.terminal.modes();
                let bytes = zyt_term::encode_paste(&text, modes);
                self.session.write(&bytes);
            }
            None => self.paste_clipboard(context),
        }
    }

    /// Starts one file operation on a thread of its own.
    pub fn start_file_task(&mut self, task: zyt_files::FileTask) {
        self.tasks.start(task, Some(self.notify.clone()));
    }

    /// Asks the dialog where a file of the session should land, which is how it
    /// is renamed as well.
    pub fn rename_file(&mut self, path: &std::path::Path) {
        self.read_dialog_memory();
        if let Some(directory) = path.parent() {
            self.file_dialog.config_mut().initial_directory = directory.to_path_buf();
        }
        if let Some(name) = path.file_name() {
            self.file_dialog.config_mut().default_file_name = name.to_string_lossy().to_string();
        }
        self.file_dialog.config_mut().title = Some(t!("file.rename_title").to_string());
        self.file_dialog.config_mut().title_bar = true;

        self.renaming = Some(path.to_path_buf());
        self.pending_pick = Some(PendingPick::Rename);
        self.file_dialog.save_file();
        self.show_view(MainView::FileDialog);
    }

    /// Moves a file into the directory the session stands in.
    pub fn move_file_here(&mut self, path: &std::path::Path) {
        let Some(directory) = self.working_directory() else {
            return;
        };
        let Some(name) = path.file_name() else {
            return;
        };
        self.start_file_task(zyt_files::FileTask::Move {
            from: path.to_path_buf(),
            to: directory.join(name),
        });
    }

    /// Reads a file into the clipboard, as far as the limit allows.
    pub fn copy_file_contents(&mut self, path: &std::path::Path) {
        self.start_file_task(zyt_files::FileTask::Read {
            path: path.to_path_buf(),
            limit: self.settings.file_menu.copy_limit,
        });
    }

    /// Reads a file and types what it holds into the session.
    ///
    /// The limit is the one of the clipboard: it is the size the application is
    /// willing to hold in memory at once, whichever way the bytes then go.
    pub fn insert_file_contents(&mut self, path: &std::path::Path) {
        let id = self.tasks.start(
            zyt_files::FileTask::Read {
                path: path.to_path_buf(),
                limit: self.settings.file_menu.copy_limit,
            },
            Some(self.notify.clone()),
        );
        self.typing_reads.insert(id);
    }

    /// Types what a file holds into the session, when it is text.
    ///
    /// A terminal takes text; bytes that are not text would be read as control
    /// sequences, so they are refused and the terminal says why.
    fn type_contents(&mut self, path: &std::path::Path, bytes: Vec<u8>) {
        let content = zyt_files::content_of(path);
        if !matches!(content, zyt_files::Content::Text(_)) {
            self.notice(t!("file.not_text_for_terminal", media = content.media_type()).to_string());
            return;
        }

        match String::from_utf8(bytes) {
            Ok(text) => self.type_text(&text),
            Err(_) => self
                .notice(t!("file.not_text_for_terminal", media = content.media_type()).to_string()),
        }
    }

    /// Hands a file or a directory to the trash, asking first when the settings
    /// of that kind of path say so.
    pub fn trash_file(&mut self, path: &std::path::Path) {
        let ask = if path.is_dir() {
            self.settings.directory_menu.trash_confirm
        } else {
            self.settings.file_menu.trash_confirm
        };
        let pending = PendingFile::Trash(path.to_path_buf());
        if ask {
            self.pending_file = Some(pending);
            return;
        }
        self.run_pending_file(pending);
    }

    /// Removes a file or a directory for good, asking first when the settings
    /// of that kind of path say so.
    pub fn delete_file(&mut self, path: &std::path::Path) {
        let ask = if path.is_dir() {
            self.settings.directory_menu.delete_confirm
        } else {
            self.settings.file_menu.delete_confirm
        };
        let pending = PendingFile::Delete(path.to_path_buf());
        if ask {
            self.pending_file = Some(pending);
            return;
        }
        self.run_pending_file(pending);
    }

    /// Runs what the question was about.
    pub fn run_pending_file(&mut self, pending: PendingFile) {
        let task = match pending {
            PendingFile::Trash(path) => zyt_files::FileTask::Trash { path },
            PendingFile::Delete(path) => zyt_files::FileTask::Delete { path },
        };
        self.start_file_task(task);
    }

    /// Where the application stands, which is the directory of this process and
    /// nothing kept beside it.
    pub fn working_directory(&self) -> Option<std::path::PathBuf> {
        std::env::current_dir().ok()
    }

    /// Asks one transfer beside the line to stop where it is.
    pub fn cancel_job(&mut self, id: zyt_xfer::JobId) {
        let outcome = self.jobs.cancel(id).map_err(AppError::from);
        self.report(outcome);
    }

    /// Takes one entry of that list away, with the file of its output.
    pub fn forget_job(&mut self, id: zyt_xfer::JobId) {
        self.jobs.remove(id);
    }

    /// Hands the file a transfer beside the line wrote to the desktop.
    ///
    /// It goes straight to the desktop and not through `open_link`: that one
    /// refuses a path an untrusted session named, and this path was never
    /// named by a session — the application made it and knows what is in it.
    pub fn open_log(&mut self, path: &std::path::Path) {
        let outcome = open::that_detached(path).map_err(|source| AppError::Link { source });
        self.report(outcome);
    }

    /// Picks up the transfers beside the line that ended and says so.
    fn handle_jobs(&mut self) {
        let ended = self.jobs.poll();
        if ended.is_empty() {
            return;
        }
        let states = self.jobs.jobs();
        for id in ended {
            let Some(state) = states.iter().find(|state| state.id == id) else {
                continue;
            };
            let took = crate::format::duration(state.elapsed());
            let command = state.title.clone();
            let text = match state.outcome {
                Some(zyt_xfer::Outcome::Done) => {
                    t!("transfer.command_finished", command = command, took = took)
                }
                Some(zyt_xfer::Outcome::Failed(Some(code))) => t!(
                    "transfer.command_failed",
                    command = command,
                    code = code,
                    took = took
                ),
                _ => t!("transfer.command_stopped", command = command, took = took),
            };
            self.notice(text.to_string());
        }
    }

    /// Picks up what the file tasks did and says so in the terminal.
    fn handle_tasks(&mut self, context: &egui::Context) {
        for event in self.tasks.poll() {
            let zyt_files::TaskEvent::Finished { id, outcome } = event else {
                continue;
            };
            match outcome {
                Ok(zyt_files::Done::Moved(path)) => {
                    self.notice(t!("file.moved", path = path.display().to_string()).to_string())
                }
                Ok(zyt_files::Done::Trashed(path)) => {
                    self.notice(t!("file.trashed", path = path.display().to_string()).to_string())
                }
                Ok(zyt_files::Done::Deleted(path)) => {
                    self.notice(t!("file.deleted", path = path.display().to_string()).to_string())
                }
                Ok(zyt_files::Done::Read { path, bytes }) => {
                    if self.typing_reads.remove(&id) {
                        self.type_contents(&path, bytes);
                    } else {
                        self.copy_to_clipboard(&path, bytes, context);
                    }
                }
                Err(zyt_files::FileError::Cancelled) => {
                    self.notice(t!("file.cancelled").to_string())
                }
                Err(error) => self.report(Err(AppError::File { source: error })),
            }
        }
    }

    /// Puts what a file holds into the clipboard, the way its type allows.
    ///
    /// A clipboard carries text and pictures; a file that is neither is named
    /// in a notice instead, because handing it over as bytes would only look
    /// like it worked.
    fn copy_to_clipboard(
        &mut self,
        path: &std::path::Path,
        bytes: Vec<u8>,
        context: &egui::Context,
    ) {
        let content = zyt_files::content_of(path);
        match &content {
            zyt_files::Content::Text(_) => match String::from_utf8(bytes) {
                Ok(text) => {
                    context.copy_text(text);
                    self.notice(t!("file.copied", media = content.media_type()).to_string());
                }
                Err(_) => {
                    self.notice(t!("file.not_text", media = content.media_type()).to_string())
                }
            },
            zyt_files::Content::Image(_) => match self.copy_image(&bytes) {
                Ok(()) => self.notice(t!("file.copied", media = content.media_type()).to_string()),
                Err(error) => {
                    log::warn!("clipboard image: {error}");
                    self.notice(t!("file.not_copied", media = content.media_type()).to_string())
                }
            },
            zyt_files::Content::Other(_) => {
                self.notice(t!("file.not_copied", media = content.media_type()).to_string())
            }
        }
    }

    /// Puts a picture into the clipboard of the platform.
    fn copy_image(&mut self, bytes: &[u8]) -> std::result::Result<(), String> {
        let image = image::load_from_memory(bytes)
            .map_err(|error| error.to_string())?
            .into_rgba8();
        let (width, height) = image.dimensions();

        let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
        clipboard
            .set_image(arboard::ImageData {
                width: width as usize,
                height: height as usize,
                bytes: std::borrow::Cow::Owned(image.into_raw()),
            })
            .map_err(|error| error.to_string())
    }

    /// Acts on a file dragged onto the window.
    ///
    /// Every session is offered the menu of that file, so a drop always asks
    /// before it does anything. What the menu holds depends on the session: a
    /// trusted one can have the path typed into its shell, and any of them can
    /// send the file with a transfer profile.
    fn handle_dropped_files(&mut self, context: &egui::Context) {
        if self.dropped.is_some() && !self.menu.is_open() {
            self.dropped = None;
        }

        let dropped = context.input(|input| input.raw.dropped_files.clone());
        let Some(path) = dropped.first().map(|file| file.path().to_path_buf()) else {
            return;
        };

        if !self.session.has_source() {
            self.notice(t!("drop.not_connected").to_string());
            return;
        }

        self.dropped = Some(path);
        self.open_drop_menu();
    }

    /// Offers what can be done with the file that was dropped.
    pub fn open_drop_menu(&mut self) {
        let items = crate::ui::menu::drop_items(self);
        if let Err(error) = self.menu.open(items) {
            log::debug!("drop menu: {error}");
            self.dropped = None;
        }
    }

    /// Types a path into the session as one quoted word.
    pub fn type_path(&mut self, path: &std::path::Path) {
        let quoted = zyt_xfer::quote_for_shell(&path.to_string_lossy());
        self.type_text(&quoted);
    }

    /// Types text into the session the way a paste is typed.
    pub fn type_text(&mut self, text: &str) {
        let modes = self.session.terminal.modes();
        let bytes = zyt_term::encode_paste(text, modes);
        self.session.write(&bytes);
    }

    /// Path of the file that was dropped, taken out of the wait.
    pub fn take_dropped(&mut self) -> Option<std::path::PathBuf> {
        self.dropped.take()
    }

    fn paste_clipboard(&mut self, context: &egui::Context) {
        self.ui.paste_pending = true;
        context.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }

    fn transfer(&mut self, direction: Direction) {
        let Some(profile) = self.active_profile().cloned() else {
            self.report(Err(AppError::NoProfile));
            return;
        };

        let commands = profile.commands(direction);
        if !commands.is_available() {
            self.report(Err(AppError::NoProfile));
            return;
        }

        let kind = commands.target_kind();
        if kind == TargetKind::None {
            self.begin_transfer(&profile, direction, Target::None);
            return;
        }

        self.read_dialog_memory();
        self.pending_pick = Some(PendingPick::Transfer(direction));
        if kind.is_directory()
            && let Some(directory) = self.remembered_directory()
        {
            self.file_dialog.config_mut().initial_directory = directory;
        }
        match (kind, direction) {
            (kind, _) if kind.is_multiple() => self.file_dialog.pick_multiple(),
            (TargetKind::Directory, _) => self.file_dialog.pick_directory(),
            (_, Direction::Send) => self.file_dialog.pick_file(),
            (_, Direction::Receive) => self.file_dialog.save_file(),
        }
        self.show_view(MainView::FileDialog);
    }

    fn remembered_directory(&self) -> Option<std::path::PathBuf> {
        let key = self.memory_key()?;
        self.memory(&key)?
            .save_directory
            .clone()
            .filter(|path| path.is_dir())
    }

    /// Asks the dialog for a path and types it into the session.
    ///
    /// The dialog says which of the two it is asking for, because the same
    /// window picks a file for a transfer and this.
    pub fn pick_path_to_type(&mut self, directory: bool) {
        self.read_dialog_memory();
        let title = if directory {
            t!("menu.pick_directory_title")
        } else {
            t!("menu.pick_file_title")
        };
        self.file_dialog.config_mut().title = Some(title.to_string());
        self.file_dialog.config_mut().title_bar = true;

        self.pending_pick = Some(PendingPick::TypePath);
        if directory {
            self.file_dialog.pick_directory();
        } else {
            self.file_dialog.pick_file();
        }
        self.show_view(MainView::FileDialog);
    }

    /// Asks for the directory a console starts in.
    pub fn pick_console_directory(&mut self, index: usize) {
        self.read_dialog_memory();
        self.file_dialog.config_mut().title = Some(t!("settings.console_directory").to_string());
        self.file_dialog.config_mut().title_bar = true;
        if let Some(console) = self.consoles.get(index) {
            let named = std::path::PathBuf::from(console.directory.trim());
            if named.is_dir() {
                self.file_dialog.config_mut().initial_directory = named;
            }
        }

        self.pending_pick = Some(PendingPick::ConsoleDirectory(index));
        self.file_dialog.pick_directory();
        self.show_view(MainView::FileDialog);
    }

    /// Writes down the directory a console starts in.
    fn set_console_directory(&mut self, index: usize, path: &std::path::Path) {
        let Some(console) = self.consoles.get_mut(index) else {
            return;
        };
        console.directory = path.display().to_string();
        self.save_console(index);
    }

    /// Acts on the path the file dialog delivered.
    pub fn take_picked_file(&mut self) {
        let Some(pending) = self.pending_pick else {
            return;
        };

        if let Some(paths) = self.file_dialog.take_picked_multiple() {
            let return_view = self.return_view;
            self.save_file_dialog();
            if let PendingPick::Transfer(direction) = pending {
                self.start_picked_transfer(direction, &paths);
            }
            self.show_view(return_view);
            return;
        }

        if let Some(path) = self.file_dialog.take_picked() {
            let return_view = self.return_view;
            self.save_file_dialog();
            match pending {
                PendingPick::Transfer(direction) => {
                    self.start_picked_transfer(direction, std::slice::from_ref(&path))
                }
                PendingPick::Rename => {
                    if let Some(from) = self.renaming.take() {
                        self.start_file_task(zyt_files::FileTask::Move { from, to: path });
                    }
                }
                PendingPick::TypePath => self.type_path(&path),
                PendingPick::ConsoleDirectory(index) => self.set_console_directory(index, &path),
            }
            self.show_view(return_view);
            return;
        }

        if !matches!(
            self.file_dialog.state(),
            egui_file_dialog::DialogState::Open
        ) {
            self.show_view(self.return_view);
        }
    }

    /// Reads what the dialog remembers, at every opening and at no other
    /// moment.
    ///
    /// The folders that are pinned, whether hidden files are listed and where
    /// the dialog stood belong to the file and not to this window: several
    /// copies of the application pin folders and walk directories of their own,
    /// and one that held its own copy would write the others' work away. So
    /// nothing is held between two openings — the file is read when the dialog
    /// opens and written when something is picked.
    fn read_dialog_memory(&mut self) {
        *self.file_dialog.storage_mut() = load_file_dialog(&self.store);
    }

    /// Writes what the dialog remembers, which is done when a path is picked
    /// and at no other moment: a dialog that was closed with nothing chosen
    /// changed nothing anybody asked to keep.
    fn save_file_dialog(&mut self) {
        let outcome = self
            .store
            .save(FILE_DIALOG_FILE, self.file_dialog.storage_mut())
            .map_err(AppError::from);
        self.report(outcome);
    }

    pub fn start_picked_transfer(&mut self, direction: Direction, paths: &[std::path::PathBuf]) {
        let Some(profile) = self.active_profile().cloned() else {
            self.report(Err(AppError::NoProfile));
            return;
        };
        let Some(first) = paths.first() else {
            return;
        };

        let kind = profile.commands(direction).target_kind();
        if kind.is_directory()
            && let Some(key) = self.memory_key()
        {
            if let Some(memory) = self.memory_mut(&key) {
                memory.save_directory = Some(first.clone());
            }
            self.save_memory(&key);
        }

        let target = match kind {
            TargetKind::Directories => Target::Directories(paths),
            TargetKind::Directory => Target::Directory(first),
            TargetKind::Files => Target::Files(paths),
            _ => Target::File(first),
        };

        self.begin_transfer(&profile, direction, target);
    }

    /// Starts a transfer the way its profile asks to be started.
    ///
    /// A profile that holds the line goes to the session, which has the line to
    /// give it and lets one of them run at a time. A profile that does not is
    /// started beside it, where nothing waits for anything: several of them run
    /// together and none of them is felt in the terminal.
    fn begin_transfer(
        &mut self,
        profile: &zyt_xfer::TransferProfile,
        direction: Direction,
        target: Target<'_>,
    ) {
        let variables = self.source_variables();
        if profile.pty {
            let outcome = self
                .session
                .start_transfer(profile, direction, target, &variables);
            self.report(outcome);
            return;
        }

        let line =
            match profile
                .commands(direction)
                .local
                .line
                .resolve(&profile.name, target, &variables)
            {
                Ok(line) => line,
                Err(error) => return self.report(Err(AppError::from(error))),
            };

        match self.jobs.start(&line, &line, Some(self.notify.clone())) {
            Ok(_) => self.notice(
                t!(
                    "transfer.started_beside",
                    profile = profile.name,
                    command = line
                )
                .to_string(),
            ),
            Err(error) => self.report(Err(AppError::from(error))),
        }
    }

    /// Draws the window with the families the settings name.
    ///
    /// It happens between two passes and never inside one: a family the
    /// toolkit does not know yet would panic while text is laid out, because
    /// new definitions only take effect in the next pass. The first window is
    /// settled in `new` for that reason — the terminal asks for its own family
    /// from the first pass on, and a pass that opened before the definitions
    /// were handed over would find nothing bound to it.
    ///
    /// The picture of the grid is thrown away on the pass after that and not on
    /// this one, for the same reason the families take effect there: a picture
    /// built here would be laid out with the families being replaced and cut
    /// against the atlas they stand in, and then kept — and the atlas it names
    /// its glyphs in is the one the next pass rebuilds. Thrown away on the pass
    /// that has the new families, it is built once, from them.
    fn apply_fonts(&mut self, context: &egui::Context) {
        if self.fonts_settling {
            self.fonts_settling = false;
            self.terminal_cache.forget();
        }
        if !self.fonts_dirty {
            return;
        }
        self.fonts_dirty = false;

        crate::fonts::apply(
            context,
            &self.settings.fonts.terminal,
            self.settings.fonts.interface.as_deref(),
        );
        self.fonts_settling = true;
        context.request_repaint();
    }

    /// Asks for another pass when the atlas of glyphs grew after the grid was
    /// painted into it.
    ///
    /// The picture of the grid is one mesh, and a mesh of text says where each
    /// glyph stands in that atlas as a share of its size: an atlas that grew
    /// moves every one of them, so a picture built against the size before is
    /// drawn with the wrong part of the image in every cell — letters that are
    /// there and are not the letters. It grows when something is laid out that
    /// was not there before, and most of that is drawn after the terminal: a
    /// panel that opened and closed again, a hint standing under the pointer.
    ///
    /// The pass that notices builds the picture again, so the whole of the fix
    /// is that there is a pass to notice in. A window with nothing to say asks
    /// for none, which is why what stood on the screen stayed wrong until a key
    /// or a resized window asked for one.
    ///
    /// A palette that changed asks for that pass outright rather than waiting
    /// to be noticed: the picture is built again there whatever the atlas did,
    /// which is [`App::settle_theme`].
    fn watch_the_atlas(&mut self, context: &egui::Context) {
        let atlas = context.fonts(|fonts| fonts.font_image_size());
        if self.theme_settling || self.terminal_cache.wants_another_pass(atlas) {
            context.request_repaint();
        }
    }

    /// Every family the system has, read once and kept.
    ///
    /// Reading them means reading the font directories of the platform, which
    /// on a machine with a thousand fonts is a pause somebody sees. So it is
    /// asked for where somebody asked for the list — the menu of the interface
    /// font, the one that adds a family to the chain of the terminal — and
    /// nowhere a panel is merely being drawn: a page that reads the fonts of
    /// the machine to open is a page that takes that pause every time it is
    /// opened, whatever it was opened for.
    pub fn font_families(&mut self) -> &[crate::fonts::Family] {
        self.font_families
            .get_or_insert_with(crate::fonts::families)
    }

    /// Tells the desktop what the program of the session says about its
    /// progress, when the desktop of the platform shows such a thing.
    ///
    /// It is said when it changes and not every frame: the taskbar of Windows
    /// is a COM call across a process boundary, and a window drawing sixty
    /// frames a second would make sixty of them to say the same thing.
    fn show_progress_outside(&mut self, frame: &eframe::Frame) {
        let progress = self.session.progress();
        if progress == self.shown_progress {
            return;
        }
        self.shown_progress = progress;
        crate::taskbar::show(frame, progress);
    }

    /// Asks for the selected font to be installed again.
    pub fn mark_fonts_dirty(&mut self) {
        self.fonts_dirty = true;
    }

    /// Acts on what the terminal reported since the last frame.
    fn handle_terminal_events(&mut self, context: &egui::Context) {
        self.handle_bell(context);

        if !self.session.terminal.modes().mouse_report {
            self.ui.mouse_reports = true;
        }

        if let Some(text) = self.session.clipboard_store.take() {
            self.stored_clipboard = text.clone();
            context.copy_text(text);
        }

        for notification in std::mem::take(&mut self.session.notifications) {
            let allowed = match notification.kind {
                zyt_term::NotificationKind::Text => self.osc().notification_text,
                zyt_term::NotificationKind::Titled => self.osc().notification_titled,
            };
            if !allowed {
                continue;
            }
            let heading = if notification.title.trim().is_empty() {
                crate::WINDOW_TITLE.to_string()
            } else {
                notification.title
            };
            if let Err(error) = notify_rust::Notification::new()
                .summary(&heading)
                .body(&notification.body)
                .show()
            {
                log::warn!("notification: {error}");
            }
        }

        if let Some(directory) = self.session.working_directory.take() {
            enter_directory(&directory);
        }

        let title = self.session.title.clone().filter(|_| self.osc().title);
        if title != self.shown_title {
            self.shown_title = title;
            self.apply_title(context);
        }

        if self.session.console_id().is_some()
            && (self.session.ended || !self.session.console_running())
        {
            self.session.ended = false;
            self.console_ended(context);
        }

        self.answer_clipboard_request(context);
    }

    /// Acts on a console whose program ended.
    ///
    /// A console that was told to come back is started again, but only when it
    /// ended the way a program that is done ends. One that failed is asked
    /// about instead: a program that cannot start fails again the moment it is
    /// started again, and a window that answers that by starting it again is a
    /// window doing nothing else — every frame spent on a program that will not
    /// run, the settings behind it replaced before they can be read. The
    /// question is the same one every other lost source asks, and it stands
    /// until it is answered.
    ///
    /// A program that left with a code of zero and was not told to come back
    /// takes the window with it, because that is a program that was done:
    /// somebody typed `exit`, and a question about what to do next would be a
    /// question about something nobody is waiting for.
    ///
    /// Anything else is a window whose source is gone in a way that was not
    /// asked for — a code that says a failure, or no code at all — and then the
    /// question stands: the output is on the screen behind it, and a window that
    /// closed itself would take that with it.
    fn console_ended(&mut self, context: &egui::Context) {
        let Some(id) = self.session.console_id() else {
            return;
        };
        // What the console is called is read off the session while it is still
        // there: the question about it stands after it has been let go of.
        let shown = self
            .session
            .console_shown()
            .map(str::to_string)
            .unwrap_or_else(|| self.console_shown(id));
        let code = self.session.console_exit_code();
        let restart = self.console(&id).is_some_and(|console| console.restart);

        if restart && code == Some(0) {
            self.restart_console(id);
            return;
        }

        if code == Some(0) {
            log::info!("the console ended cleanly, the window leaves with it");
            self.session.disconnect();
            context.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        log::info!("the console ended with {code:?}, asking what to do about it");
        self.session.disconnect();
        self.ask_about_lost(LostSource {
            source: SourceSetting::Console { id },
            reason: LostReason::Ended { code },
            shown,
        });
    }

    /// Starts a console that was told to come back, without taking the window
    /// away from whatever is being read in it.
    ///
    /// Opening a source shows the terminal, because opening one is what
    /// somebody asked for. Nobody asked for this one: the console came back by
    /// itself, and a window standing in the settings when it did is a window
    /// whose settings were being read.
    fn restart_console(&mut self, id: ConsoleId) {
        let view = self.ui.view;
        let shown = self.console_shown(id);
        self.notice(t!("console.restarting", name = shown).to_string());
        let outcome = self.connect_console(id);
        let started = outcome.is_ok();
        self.report(outcome);
        if started && view != MainView::Terminal {
            self.show_view(view);
        }
    }

    /// Hands the parts of the OSC settings that the lower layers enforce down
    /// to them: the clipboard access of the terminal and the shell marks of the
    /// session.
    ///
    /// A sequence the settings refuse is ignored, never answered and never
    /// reported, so the terminal enforces every clipboard choice itself and a
    /// program that asks for more than it may have is left without an answer.
    pub fn apply_osc_settings(&mut self) {
        let access = match self.osc().clipboard {
            crate::config::ClipboardSetting::Disabled => zyt_term::ClipboardAccess::Disabled,
            crate::config::ClipboardSetting::CopyOnly => zyt_term::ClipboardAccess::CopyOnly,
            crate::config::ClipboardSetting::CopyPaste
            | crate::config::ClipboardSetting::CopyLimitedPaste => {
                zyt_term::ClipboardAccess::CopyPaste
            }
        };
        self.session.set_clipboard_access(access);
        self.session.marks_enabled = self.osc().marks;
        self.session.progress_enabled = self.osc().progress;
    }

    /// Draws the interface at the size the settings ask for.
    ///
    /// The number is the size of the body text, and every other text of the
    /// interface follows it by the share it stands at in the sizes of the
    /// toolkit: a heading stays larger than the text under it and a hint stays
    /// smaller, because what is being changed is how large the interface is
    /// read at and not which text is which.
    ///
    /// Only the text is given a size. A control of the toolkit is as large as
    /// what stands in it, so it follows by itself, and the room between two of
    /// them is not text and does not.
    ///
    /// The sizes are worked out from the ones the toolkit ships with every
    /// time, never from the ones the window is wearing: a factor applied to
    /// what a factor was already applied to is a window that grows a little
    /// more with every frame.
    pub fn apply_interface_size(&self, context: &egui::Context) {
        let factor = self.settings.interface_font_size / crate::config::DEFAULT_INTERFACE_FONT_SIZE;
        let sizes: std::collections::BTreeMap<egui::TextStyle, egui::FontId> =
            egui::style::default_text_styles()
                .into_iter()
                .map(|(style, font)| {
                    let size = (font.size * factor).max(1.0);
                    (style, egui::FontId { size, ..font })
                })
                .collect();

        context.all_styles_mut(|style| style.text_styles = sizes.clone());
    }

    /// Tells the toolkit how long a hint waits before it is shown.
    ///
    /// The wait is what costs: while it runs, the toolkit asks for a frame
    /// after every movement of the pointer, waiting for it to come to rest. A
    /// delay of nothing shows the hint at once and asks for nothing.
    pub fn apply_tooltip_delay(&self, context: &egui::Context) {
        let delay = self.settings.tooltip_delay;
        context.all_styles_mut(|style| {
            style.interaction.tooltip_delay = delay.unwrap_or(0.0);
            style.interaction.show_tooltips_only_when_still = delay.is_some();
        });
    }

    /// Answers a clipboard request of the program.
    ///
    /// The clipboard belongs to the toolkit, so the answer takes two steps: the
    /// toolkit is asked for the text and the answer goes out when its paste
    /// event arrives. Whether a request is answered at all is decided by the
    /// clipboard setting, which the terminal enforces, so a request reaches
    /// this point only when reading is allowed.
    ///
    /// The limited setting answers in one step and never asks the toolkit: what
    /// a program of this session stored is what it reads back, and the clipboard
    /// of the desktop is not opened to it at all.
    fn answer_clipboard_request(&mut self, context: &egui::Context) {
        if !self.session.clipboard_requested {
            self.ui.clipboard_stage = ClipboardStage::Idle;
            return;
        }

        if self.osc().clipboard == crate::config::ClipboardSetting::CopyLimitedPaste {
            let stored = self.stored_clipboard.clone();
            self.ui.clipboard_stage = ClipboardStage::Idle;
            self.session.clipboard_requested = false;
            self.session.terminal.answer_clipboard(Some(&stored));
            self.session.flush_terminal_output();
            return;
        }

        if self.ui.clipboard_stage == ClipboardStage::Waiting {
            let waited = self
                .ui
                .clipboard_asked
                .is_some_and(|asked| asked.elapsed() > Duration::from_millis(400));
            if waited {
                log::debug!("no clipboard from the toolkit, answering with nothing");
                self.ui.clipboard_stage = ClipboardStage::Idle;
                self.session.clipboard_requested = false;
                self.session.terminal.answer_clipboard(Some(""));
                self.session.flush_terminal_output();
            }
            context.request_repaint();
            return;
        }

        self.ui.clipboard_stage = ClipboardStage::Waiting;
        self.ui.clipboard_asked = Some(Instant::now());
        context.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        context.request_repaint();
    }

    /// Remembers a selection the way the platform expects it.
    pub fn publish_selection(&mut self, text: &str, context: &egui::Context) {
        if crate::primary::Primary::exists() {
            self.primary.set(text);
        } else {
            context.copy_text(text.to_string());
        }
    }

    /// Opens a link with the program the desktop picked for it.
    pub fn open_link(&mut self, uri: &str) {
        if self.refuses_path(uri) {
            return;
        }
        let outcome = open::that_detached(uri).map_err(|source| AppError::Link { source });
        self.report(outcome);
    }

    /// Asks the desktop which program should open a link.
    pub fn open_link_with(&mut self, uri: &str) {
        if self.refuses_path(uri) {
            return;
        }
        let outcome = crate::openwith::open_with(uri).map_err(|source| AppError::Link { source });
        self.report(outcome);
    }

    /// True while this address names a path of this machine and the session
    /// that printed it is not trusted.
    ///
    /// Such an address is that session's word for a file that is here, and
    /// opening it would start whatever the desktop runs for that type — from a
    /// click as much as from a menu — so it is refused wherever it comes from,
    /// and the terminal says why.
    fn refuses_path(&mut self, uri: &str) -> bool {
        if self.session_is_trusted() || zyt_files::path_of(uri).is_none() {
            return false;
        }
        self.notice(t!("link.not_opened").to_string());
        true
    }

    /// Asks for the next frame only where one is needed.
    ///
    /// Device output, connection events and transfer progress wake the window
    /// through the notify callback of the worker threads, so an idle terminal
    /// costs no frames at all. A running transfer shows progress, so it keeps a
    /// slow tick. So does a running file task: it has to be drawn again to show
    /// the time it has been running, and to notice that it has stopped, because
    /// a window that nothing else wakes would keep a finished task on the bar.
    fn schedule_repaint(&mut self, context: &egui::Context) {
        if let Some(left) = self.session.next_read() {
            context.request_repaint_after(left);
        }
        let wake_after = self
            .read_interval()
            .map_or(0, |delay| delay.as_micros() as u64);
        self.wake_delay
            .store(wake_after, std::sync::atomic::Ordering::Relaxed);

        let interval = if self.session.is_transferring() {
            Some(Duration::from_millis(50))
        } else if !self.tasks.is_empty() || self.jobs.running() > 0 {
            Some(Duration::from_millis(200))
        } else {
            None
        };
        if let Some(interval) = interval {
            context.request_repaint_after(interval);
        }
    }

    /// Opens the menu of the sources the moment a session ends.
    ///
    /// It is the moment and not the state: the terminal stays where it is
    /// whether anything is connected or not, so a window with no source is a
    /// window showing what the last one left, and asking the list again every
    /// frame would be a menu nobody could close. The list is asked for and not
    /// opened here, because a session may end while the settings are being read
    /// in the window and a menu over them is not what that asked for.
    ///
    /// The scrollback of the session that ended is dropped as it ends: the
    /// lines above the screen belong to something that is over, and a window
    /// left standing with no source would hold every one of them — up to the
    /// whole budget — for as long as it lives. What the screen shows stays
    /// where it is.
    ///
    /// A question about a source the window cannot reach is the one thing being
    /// answered while it stands, so the list is not offered beside it: it is
    /// where that question leads when it is answered that way, and not before.
    fn follow_connection(&mut self) {
        let has_source = self.session.has_source();
        if self.lost.is_some() {
            self.had_source = has_source;
            return;
        }
        if self.had_source && !has_source {
            self.session.terminal.forget_scrollback();
            self.source_menu_wanted = true;
        }
        self.had_source = has_source;
    }

    /// The shortest time between two readings of the source, as the ladder of
    /// the settings asks for it at the speed the source is running at.
    ///
    /// A running transfer is never held back: its program answers a block and
    /// waits for the next one, so bytes left lying are a transfer standing
    /// still.
    pub fn read_interval(&mut self) -> Option<Duration> {
        let speed = self.session.byte_rate();
        read_interval_at(&self.settings.read_steps, self.settings.read_above, speed)
            .filter(|_| !self.session.is_transferring())
    }

    /// Tells the grid how many lines its budget buys at the width it has now.
    ///
    /// The budget is memory and a row costs its full width, so the number of
    /// lines follows the window: widening it drops the oldest lines rather than
    /// quietly costing more, and narrowing it keeps more of them. It is asked on
    /// every frame because every frame is where the width may have changed, and
    /// the grid does nothing at all when the number it is told is the one it has.
    pub fn apply_scrollback(&mut self) {
        let columns = self.session.terminal.size().0;
        let lines = crate::config::scrollback_lines(self.settings.scrollback_memory, columns);
        self.session.terminal.set_scrollback(lines);
    }

    /// Tells the session how long the bytes of a source may wait.
    /// Tells the session how long the bytes wait at the speed the source is
    /// running at now.
    ///
    /// It is asked on every frame, because the speed is what decides it and the
    /// speed is a thing of the last second. A ladder that answers the same step
    /// as the frame before costs the session nothing.
    pub fn apply_read_interval(&mut self) {
        let interval = self.read_interval();
        self.session.set_read_interval(interval);
    }

    /// Tells the session how large the buffer of a source is, which the
    /// settings say in kibibytes and the session in bytes.
    pub fn apply_read_buffer(&mut self) {
        let size = self
            .settings
            .read_buffer
            .map(|kibibytes| kibibytes.saturating_mul(1024));
        self.session.set_read_buffer(size);
    }

    /// Opens the menu of the sources once the window is there.
    ///
    /// Listing the sources walks sysfs and the udev database, which is longer
    /// than a frame and longer than a window should take to appear. A window
    /// that starts on nothing is exactly the one that wants that list, so the
    /// menu waits for the frame that shows the window and stands a frame later.
    ///
    /// It waits for the terminal as well: the settings and the file dialog are
    /// each the one thing being done while they stand, and a list of sources
    /// over them is not what was asked for. It comes up when the window is back
    /// on the terminal, which is where a source would be opened anyway — unless
    /// a source was opened meanwhile, and then there is nothing to ask.
    fn open_source_menu_when_shown(&mut self, context: &egui::Context) {
        if !self.source_menu_wanted {
            return;
        }
        if self.session.has_source() {
            self.source_menu_wanted = false;
            return;
        }
        if !self.shown {
            context.request_repaint();
            return;
        }
        if self.ui.view != MainView::Terminal || self.ui.ask.is_some() {
            return;
        }

        self.source_menu_wanted = false;
        self.open_source_menu();
    }

    /// Opens the menu of the sources this window can connect to.
    ///
    /// Every source carries the moment it was last opened on the plate beside
    /// the menu, so the room for that plate is kept whatever the selection
    /// stands on: a plate that comes and goes as the list is walked says less
    /// than one that is always there. The ports the process may not open are
    /// counted in the line above the entries, because a device that is there
    /// and cannot be opened is not a device that is missing.
    pub fn open_source_menu(&mut self) {
        self.refresh_sources();
        let items = crate::ui::connect::items(self);
        self.menu.shift(plate_menu::Shift::Always);
        if let Err(error) = self.menu.open(items) {
            log::debug!("source menu: {error}");
            return;
        }
        if self.hidden_ports > 0 {
            self.menu
                .notice(t!("ports.hidden", count = self.hidden_ports));
        }
        self.source_menu = true;
        self.sources_listed = Some(Instant::now());
    }

    /// Lists the sources again while their menu stands, once a second.
    ///
    /// A device is plugged in and unplugged while the list is being read, so
    /// the list is read again; a scan walks sysfs and the udev database, so it
    /// is read again once a second and not once a frame. Nothing is listed
    /// while the menu is closed: that is what a scan behind a view that shows
    /// no sources would be.
    fn list_sources_again(&mut self, context: &egui::Context) {
        if !self.source_menu {
            return;
        }
        if !self.menu.is_open() {
            self.source_menu = false;
            self.sources_listed = None;
            return;
        }

        let listed = self.sources_listed.unwrap_or_else(std::time::Instant::now);
        let left = SOURCES_INTERVAL.saturating_sub(listed.elapsed());
        if !left.is_zero() {
            context.request_repaint_after(left);
            return;
        }

        self.refresh_sources();
        let items = crate::ui::connect::items(self);
        if let Err(error) = self.menu.refill(items) {
            log::debug!("source menu: {error}");
        }
        self.sources_listed = Some(Instant::now());
        context.request_repaint_after(SOURCES_INTERVAL);
    }

    /// Reads the consoles, what is remembered about the ports and the ports of
    /// the machine, all three from where they really are.
    pub fn refresh_sources(&mut self) {
        self.consoles = crate::consoles::load_all(&self.store);
        self.ports_memory = crate::sources::load_ports(&self.store);
        self.rescan_ports();
    }

    /// Opens the half of the settings about the connection this window is on.
    ///
    /// It is what the menu of the session walks into. That menu is about one
    /// source, so the settings it opens are about that source, and the page is
    /// put back on it whatever was last picked there.
    pub fn open_connection_settings(&mut self) {
        self.read_sources_again();
        self.settings_tab = SettingsTab::Connection;
        self.show_view(MainView::Settings);
    }

    /// Reads the consoles and the devices from disk again and shows the one
    /// this window is on.
    ///
    /// The files are a list somebody edits — with this program, with another
    /// copy of it, with an editor — so a page about one of them opens on what
    /// stands there now and not on what stood there when the window started.
    /// The pick goes with it: a page of connection settings is opened to look
    /// at the connection, and the one in hand is the one in front of the user.
    pub fn read_sources_again(&mut self) {
        self.refresh_sources();
        self.settings_source = None;
    }

    /// Rescans the serial ports now.
    ///
    /// A scan walks sysfs and the udev database, so it is run while the menu of
    /// the sources stands — once a second, no oftener — and when the user asks
    /// for it, never on a timer behind a window that is showing none of them.
    pub fn rescan_ports(&mut self) {
        match zyt_serial::available_ports() {
            Ok(ports) => {
                let total = ports.len();
                self.ports = ports.into_iter().filter(|port| port.accessible).collect();
                self.hidden_ports = total - self.ports.len();
            }
            Err(error) => log::warn!("port scan failed: {error}"),
        }
    }

    /// Which contexts the key bindings are resolved in.
    ///
    /// A menu of plates holds the keyboard alone as well, and before a transfer
    /// does: it stands over whatever it was opened from, and a key the menu walks
    /// by must not also be a binding of the view behind it.
    ///
    /// A transfer holds the keyboard alone: whatever is typed while it runs
    /// would land in the middle of a protocol, so the only binding that stands
    /// is the one that stops it.
    fn active_contexts(&self) -> &'static [&'static str] {
        if self.menu.is_open() {
            return &[zyt_keymux::CONTEXT_PALETTE];
        }
        if self.session.is_transferring() {
            return &[CONTEXT_TRANSFER];
        }
        match self.ui.focus {
            Focus::Terminal => &[CONTEXT_TERMINAL],
            Focus::Settings => &[CONTEXT_SETTINGS],
            Focus::FileDialog => &[CONTEXT_FILE_DIALOG],
            Focus::StatusBar => &[zyt_keymux::CONTEXT_STATUS_BAR],
            Focus::Search => &[zyt_keymux::CONTEXT_SEARCH],
        }
    }

    /// Tells the dispatcher which contexts the bindings are resolved in, when
    /// what decides them has changed.
    ///
    /// It is settled before a key is read and whether or not any is: a menu that
    /// opened takes the bindings of the view behind it out of reach, and the
    /// palette lists what can be reached, so the two must not disagree for a
    /// frame.
    fn settle_contexts(&mut self) {
        let transferring = self.session.is_transferring();
        let menu_open = self.menu.is_open();
        if self.last_focus == self.ui.focus
            && self.last_transferring == transferring
            && self.last_menu_open == menu_open
        {
            return;
        }
        self.last_focus = self.ui.focus;
        self.last_transferring = transferring;
        self.last_menu_open = menu_open;
        self.dispatcher.set_contexts(self.active_contexts());
    }

    /// Reads the keyboard once, resolves what is bound and passes the rest on.
    ///
    /// A transfer takes no keys at all: the device behind it is in the middle
    /// of a protocol, and a byte typed into that would be read as part of it.
    ///
    /// A window that is being answered — a question about something to be
    /// deleted, a menu of plates, the window asking for the values of a
    /// connection — takes the keyboard whole. What is typed into a field of it
    /// is an answer to that window and nothing else: read here as well, the
    /// letters of a host name would be sent to the device standing behind it.
    fn handle_keyboard(&mut self, context: &egui::Context) {
        self.settle_contexts();
        if self.ui.pending_delete.is_some() || self.menu.is_open() || self.ui.ask.is_some() {
            return;
        }

        let events = context.input(|input| input.events.clone());
        let mut commands = Vec::new();
        let mut bytes = Vec::new();

        for event in events {
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if let Some(stroke) = crate::keys::to_stroke(key, &modifiers) {
                        match self.dispatcher.press(stroke) {
                            Dispatch::Command(id) => {
                                commands.push(id);
                                continue;
                            }
                            Dispatch::Pending => continue,
                            Dispatch::Unhandled => {}
                        }
                    }
                    if self.ui.focus == Focus::Terminal {
                        self.encode_terminal_key(key, &modifiers, &mut bytes);
                    }
                }
                egui::Event::Text(text) => {
                    if self.ui.focus == Focus::Terminal
                        && !context.input(|input| {
                            let modifiers = input.modifiers;
                            modifiers.ctrl || modifiers.command || modifiers.alt
                        })
                    {
                        bytes.extend_from_slice(text.as_bytes());
                    }
                }
                egui::Event::Copy => {
                    self.clipboard_shortcut('c', None, context, &mut commands, &mut bytes)
                }
                egui::Event::Cut => {
                    self.clipboard_shortcut('x', None, context, &mut commands, &mut bytes)
                }
                egui::Event::Paste(text) if self.ui.clipboard_stage == ClipboardStage::Waiting => {
                    self.ui.clipboard_stage = ClipboardStage::Idle;
                    self.session.clipboard_requested = false;
                    self.session.terminal.answer_clipboard(Some(&text));
                }
                egui::Event::Paste(text) if self.ui.paste_pending => {
                    self.ui.paste_pending = false;
                    let modes = self.session.terminal.modes();
                    bytes.extend(zyt_term::encode_paste(&text, modes));
                }
                egui::Event::Paste(text) => {
                    self.clipboard_shortcut('v', Some(&text), context, &mut commands, &mut bytes)
                }
                _ => {}
            }
        }

        for id in commands {
            if let Some(command) = AppCommand::from_id(&id) {
                self.run_command(command, context);
            }
        }
        if bytes.is_empty() {
            return;
        }
        self.session.terminal.scroll_to_bottom();
        if !self.session.is_transferring() {
            self.session.write(&bytes);
        }
    }

    /// Handles the clipboard shortcuts.
    ///
    /// The window toolkit turns them into clipboard events and swallows the key
    /// press, so the press is rebuilt here and offered to the key bindings like
    /// any other combination. What no binding claims reaches the terminal.
    fn clipboard_shortcut(
        &mut self,
        key: char,
        pasted: Option<&str>,
        context: &egui::Context,
        commands: &mut Vec<CommandId>,
        bytes: &mut Vec<u8>,
    ) {
        let modifiers = context.input(|input| input.modifiers);
        let stroke = KeyStroke {
            code: zyt_keymux::KeyCode::Char(key),
            modifiers: zyt_keymux::Modifiers {
                shift: modifiers.shift,
                ctrl: true,
                alt: modifiers.alt,
                meta: false,
            },
        };

        match self.dispatcher.press(stroke) {
            Dispatch::Command(id) => {
                if let (Some(text), true) = (pasted, id.as_str() == AppCommand::Paste.id()) {
                    let modes = self.session.terminal.modes();
                    bytes.extend(zyt_term::encode_paste(text, modes));
                } else {
                    commands.push(id);
                }
            }
            Dispatch::Pending => {}
            Dispatch::Unhandled => {
                if self.ui.focus != Focus::Terminal {
                    return;
                }
                let modes = self.session.terminal.modes();
                let encoded = zyt_term::encode_key(
                    &zyt_term::Key::Char(key),
                    zyt_term::Modifiers {
                        shift: modifiers.shift,
                        ctrl: true,
                        alt: modifiers.alt,
                    },
                    modes,
                );
                if let Some(encoded) = encoded {
                    bytes.extend(encoded);
                }
            }
        }
    }

    fn encode_terminal_key(
        &self,
        key: egui::Key,
        modifiers: &egui::Modifiers,
        bytes: &mut Vec<u8>,
    ) {
        let Some(mapped) = zyt_term_egui::map_key(key) else {
            return;
        };
        let mods = zyt_term_egui::map_modifiers(modifiers);
        if matches!(mapped, zyt_term::Key::Char(_)) && !mods.ctrl && !mods.alt {
            return;
        }
        let modes = self.session.terminal.modes();
        if let Some(encoded) = zyt_term::encode_key(&mapped, mods, modes) {
            bytes.extend(encoded);
        }
    }
}

impl eframe::App for App {
    /// Cuts the texture side the toolkit is told about down to what the glyphs
    /// need.
    ///
    /// The window asks the chip for the largest texture it grants, because a
    /// texture that may be created is worth having; the atlas the toolkit cuts
    /// its glyphs from is another matter. Its rows are as wide as this number and
    /// its height doubles as glyphs arrive, so every step of that doubling costs
    /// the full width twice over — once here and once on the chip — and a row of
    /// sixteen thousand pixels is a row nothing asked for.
    fn raw_input_hook(&mut self, _context: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(side) = raw_input.max_texture_side.as_mut() {
            *side = (*side).min(crate::render::ATLAS_SIDE);
        }
    }

    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_started = Some(Instant::now());
        self.settle_theme();
        self.apply_read_interval();
        self.apply_scrollback();
        if self.theme.poll() {
            context.set_visuals(self.theme.visuals());
            self.apply_terminal_theme();
        }
        self.follow_connection();
        self.open_source_menu_when_shown(context);
        self.list_sources_again(context);
        self.apply_fonts(context);

        for error in self.session.pump() {
            self.report(Err(error));
        }
        self.write_down_commands();
        self.handle_keyboard(context);
        self.handle_terminal_events(context);
        self.handle_dropped_files(context);
        self.handle_tasks(context);
        self.handle_jobs();

        self.schedule_repaint(context);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        ui::draw(self, ui);
        self.watch_the_atlas(ui.ctx());
        self.show_progress_outside(frame);
        self.shown = true;
        if let Some(started) = self.frame_started.take() {
            self.meter.drew(started.elapsed());
        }
    }
}

/// Makes a directory the one this process stands in.
///
/// Everything that asks where the application is — the console it opens, the
/// file dialog, a dropped file, a window started from this one — reads the
/// directory of the process, so this is the only thing that has to move: a
/// console profile that names a directory moves it at the connection, and OSC 7
/// moves it whenever the shell says it went somewhere else. A directory that
/// cannot be entered is a line in the log and nothing else.
fn enter_directory(directory: &std::path::Path) {
    if let Err(error) = std::env::set_current_dir(directory) {
        log::debug!("working directory {}: {error}", directory.display());
    }
}

/// What the file dialog remembered the last time, or nothing on the first run.
///
/// The shape belongs to the dialog and travels with its version, so a file it
/// cannot read is a line in the log and a fresh start, never a refusal to open
/// the window.
fn load_file_dialog(store: &ConfigStore) -> egui_file_dialog::FileDialogStorage {
    match store.load::<egui_file_dialog::FileDialogStorage>(FILE_DIALOG_FILE) {
        Ok(Some(storage)) => storage,
        Ok(None) => egui_file_dialog::FileDialogStorage::default(),
        Err(error) => {
            log::warn!("file dialog: {error}");
            egui_file_dialog::FileDialogStorage::default()
        }
    }
}

fn load_keymap(store: &ConfigStore) -> Result<Keymap> {
    let mut keymap = default_keymap()?;
    match store.load::<Keymap>(KEYMAP_FILE)? {
        Some(user) => keymap.merge(user),
        None => store.save(KEYMAP_FILE, &keymap)?,
    }
    Ok(keymap)
}

/// Diagnostic text of an error and of every cause below it.
struct Detail<'a>(&'a AppError);

impl std::fmt::Display for Detail<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut source: Option<&dyn std::error::Error> = std::error::Error::source(self.0);
        let mut first = true;
        while let Some(error) = source {
            if !first {
                formatter.write_str(": ")?;
            }
            write!(formatter, "{error}")?;
            first = false;
            source = error.source();
        }
        Ok(())
    }
}

/// Takes the key that leaves the application back from egui, which binds
/// `ctrl+q` on its own and closes the root viewport with it.
///
/// Here that key is XON, the answer a stopped terminal waits for, and it is
/// owed to the console and to the port. Leaving is asked for by name:
/// `app.quit` stands in the palette and carries no key until somebody binds
/// one.
fn release_quit_key(context: &egui::Context) {
    context.options_mut(|options| options.quit_shortcuts.clear());
}

/// Key sequence bound to a command, for display next to a menu entry.
pub fn shortcut_text(dispatcher: &KeyDispatcher, command: AppCommand) -> Option<String> {
    let contexts = [
        zyt_keymux::Context::new(CONTEXT_TERMINAL),
        zyt_keymux::Context::new(zyt_keymux::Context::GLOBAL),
    ];
    dispatcher
        .keymap()
        .keys_for(&contexts, &CommandId::new(command.id()))
        .map(|chord| chord.to_text())
}

/// Translated text for a key sequence bound to a command.
pub fn shortcut_or_empty(dispatcher: &KeyDispatcher, command: AppCommand) -> String {
    shortcut_text(dispatcher, command).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_wait_takes_what_arrived_whenever_a_frame_asks() {
        assert_eq!(read_interval(None), None);
        assert_eq!(read_interval(Some(0)), None);
    }

    #[test]
    fn a_wait_is_the_shortest_time_between_two_readings() {
        assert_eq!(read_interval(Some(16)), Some(Duration::from_millis(16)));
        assert_eq!(read_interval(Some(100)), Some(Duration::from_millis(100)));
    }

    #[test]
    fn an_open_search_bar_keeps_the_keyboard_from_the_terminal() {
        assert_eq!(Focus::Terminal.with_search(true), Focus::Search);
        assert_eq!(Focus::Terminal.with_search(false), Focus::Terminal);
    }

    #[test]
    fn an_open_search_bar_leaves_every_other_part_alone() {
        for focus in [
            Focus::Settings,
            Focus::FileDialog,
            Focus::StatusBar,
            Focus::Search,
        ] {
            assert_eq!(focus.with_search(true), focus);
            assert_eq!(focus.with_search(false), focus);
        }
    }
}
