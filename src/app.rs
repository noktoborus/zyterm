//! Application state and frame loop.

use crate::commands::{AppCommand, build_registry};
use crate::config::{
    FILE_DIALOG_FILE, HistoryAction, KEYMAP_FILE, SETTINGS_FILE, Settings, SourceSetting,
};
use crate::consoles::{Console, ConsoleId};
use crate::error::{AppError, Result};
use crate::session::{Purpose, Session};
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
use zyt_script::{Direction, Target, Targets};
use zyt_serial::{PortId, PortInfo};
use zyt_term::SelectionStep as Step;
use zyt_term::{Command, SearchDirection};
use zyt_term_egui::{TerminalFont, TerminalTheme};

/// Context active while the file dialog is shown.
const CONTEXT_FILE_DIALOG: &str = "file_dialog";

/// Name the dialog offers for the file a selection is saved into.
///
/// A selection is text, and text is what the desktop reads from a file of that
/// suffix. The dialog is where it is changed, and where it is remembered from one
/// save to the next.
const SELECTION_FILE: &str = "selection.txt";

/// Name a picture of a program is offered under when it is saved.
///
/// It is a PNG and not the payload as it arrived, because a file is opened by
/// whatever the desktop has for its kind, and what a desktop has for sixel is
/// nothing. The payload is kept all the same — it is what the picture is
/// decoded from every time it is saved or copied.
const PICTURE_FILE: &str = "picture.png";

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingPick {
    /// Paths a running script asked for (`zyt.ui.pick`); the question waits
    /// in [`App::picking`].
    Script,
    /// Where a file of the session should land.
    Rename,
    /// A path to type into the session.
    TypePath,
    /// A file whose contents are typed into the session.
    InsertContents,
    /// Where something the window holds — the selected text, a picture a
    /// program sent — should land.
    SaveFile {
        /// Whether the desktop is then asked which program opens it.
        open_with: bool,
    },
    /// The directory a console starts in, for the console of that place in the
    /// list.
    ConsoleDirectory(usize),
    /// A path one row of the window a script asks with stands on, by the name
    /// of that row.
    FormPath(String),
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
    /// The dialog a script is waiting on, while one stands.
    pub form: Option<crate::ui::form::State>,
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
    /// The command of several lines being typed, while that window stands.
    ///
    /// It is the window's own copy: what the field holds is not in the device
    /// and not in the history until it is sent, so closing the window is
    /// dropping it.
    pub block: Option<String>,
    /// True while that plate stands at the head of the terminal rather than at
    /// its foot.
    ///
    /// The button under the field moves it, and the choice is kept for as long
    /// as the window runs and no longer: it is answered by what the output is
    /// doing now, which is what a setting written to a file cannot know.
    pub block_at_top: bool,
    /// True while the carets of that plate are read as control codes.
    ///
    /// The switch beside the button that sends turns it. It is the switch of
    /// the source of this window and not of the window: it is written to
    /// `SourceMemory::block_caret` as it is turned and read back from there
    /// when the plate opens, because what a caret means is a fact about what is
    /// at the other end of the line. This is where it stands while the plate
    /// does, so a block of text with carets in it is still sent by turning the
    /// switch off for that block.
    pub block_caret: bool,
    /// True once that plate has been drawn at least one frame.
    ///
    /// The frame that opens it reads the keyboard before it is drawn, so the
    /// press that opened it is still in the events when the plate first
    /// stands. A plate that answered keys on its first frame would answer that
    /// one, and the key that opens it would close it again on the same frame.
    pub block_drawn: bool,
    /// True while the plate of the sign of trust stands whether or not the
    /// pointer is on that sign.
    ///
    /// The right button on the sign turns it over, because the left one is
    /// already the switch of trust.
    pub trust_plate_pinned: bool,
    /// True while that plate is held down under a pointer that would otherwise
    /// be raising it, until that pointer leaves.
    pub trust_plate_hidden: bool,
    /// Where it stood the last time it was drawn, so a click on it is told from
    /// a click beside it.
    pub trust_plate_rect: egui::Rect,
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
    /// The corner of the terminal the plate of the selection stands in.
    ///
    /// It is decided while the selection is being made and kept once the button
    /// is let go of: a plate that walked to another corner as the pointer moved
    /// away would move while nothing about the selection changed. It is dropped
    /// with the selection, so the next one picks a corner of its own.
    pub selection_corner: Option<egui::Align2>,
    /// The samples that plate last drew, kept so that copying them out of the
    /// worker allocates once and not once a frame.
    pub signal_samples: Vec<zyt_serial::LineSample>,
    /// The one value of the settings that is open for writing, if any.
    pub editing: crate::ui::widgets::Editing,
    /// The window asking for the values a console needs, while it stands.
    pub ask: Option<Ask>,
    /// The question of the new name of a window of tmux, while it stands.
    pub tmux_rename: Option<crate::ui::tmux::Rename>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: MainView::Terminal,
            focus: Focus::Terminal,
            form: None,
            mouse_reports: true,
            clipboard_stage: ClipboardStage::Idle,
            clipboard_asked: None,
            paste_pending: false,
            status_bar_focus_wanted: false,
            status_bar_id: None,
            data_plate_pinned: false,
            data_plate_hidden: false,
            data_plate_rect: egui::Rect::NOTHING,
            block: None,
            block_at_top: false,
            block_caret: false,
            block_drawn: false,
            trust_plate_pinned: false,
            trust_plate_hidden: false,
            trust_plate_rect: egui::Rect::NOTHING,
            tasks_button_rect: egui::Rect::NOTHING,
            tasks_hovered: false,
            signals_hovered: false,
            signals_pinned: false,
            signals_hidden: false,
            selection_corner: None,
            signal_samples: Vec::new(),
            editing: crate::ui::widgets::Editing::default(),
            ask: None,
            tmux_rename: None,
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

/// The picture a menu standing open is about.
///
/// The number is carried beside the picture because an entry that changes
/// something about it — folding it into its line and unfolding it again — says
/// so to the terminal, which knows a picture by its number.
#[derive(Clone)]
pub struct PictureMenu {
    /// The number the cells of it carry.
    pub id: zyt_term::ImageId,
    /// The picture as it stood when the menu opened.
    pub picture: std::sync::Arc<zyt_term::Picture>,
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
    /// Picture the menu standing open is about.
    ///
    /// It is taken when the menu opens and held while it stands, so an entry
    /// chosen a minute later acts on the picture the menu was opened on and not
    /// on whatever its number names by then.
    pub image_menu: Option<PictureMenu>,
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
    /// What each source was answered on the way in, this run of the window.
    ///
    /// The window that asks before a console is opened takes values the
    /// settings of that source do not answer, and the console is started with
    /// them. They are what the connection stands on, so everything that asks
    /// the source for a value by name is answered from here as well: a script
    /// that wants `remote_host` is offered on a connection that was told one,
    /// and not only on a source whose settings carry it.
    ///
    /// It is kept here and not written into the settings of the source: a
    /// value in the settings is a decision about the console, and one of these
    /// is what somebody typed on the way in. The file of `answers` is where
    /// they outlive the run.
    answered: std::collections::BTreeMap<SourceKey, std::collections::BTreeMap<String, String>>,
    /// Terminal font.
    pub font: TerminalFont,
    /// Shapes of the grid, kept between frames.
    pub terminal_cache: zyt_term_egui::TerminalCache,
    /// The pictures of the panes of tmux drawn beside the one shown.
    pub beside_caches: std::collections::HashMap<zyt_tmux::PaneId, zyt_term_egui::TerminalCache>,
    /// The scripts of this machine, found in the directories searched.
    pub scripts: zyt_script::Library,
    /// Query and modes of the search bar.
    pub search: crate::search::SearchState,
    /// Key bindings.
    pub dispatcher: KeyDispatcher,
    /// Commands of the palette.
    pub registry: CommandRegistry,
    /// Menu of plates: the context menu, the link menu, the palette and the
    /// transfer control all open this one widget.
    pub menu: plate_menu::PlateMenu,
    /// The history files already read, each answered from here until it is
    /// written again.
    history: crate::history::Cache,
    /// File operations, each on a thread of its own.
    pub tasks: zyt_files::TaskRunner,
    /// The transfer programs running beside the line, each with its own file
    /// of output.
    pub jobs: zyt_script::JobRunner,
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
    /// The dialog of the file system a running script waits on, while the
    /// dialog stands.
    pub picking: Option<crate::scripts::Picking>,
    /// Source the window cannot reach, waiting for the question about it to be
    /// answered.
    pub lost: Option<LostSource>,
    /// File dropped onto the window, waiting for the menu to say what to do
    /// with it.
    pub dropped: Option<Vec<std::path::PathBuf>>,
    /// Reads whose bytes go into the session instead of the clipboard.
    typing_reads: std::collections::BTreeSet<zyt_files::TaskId>,
    /// Writes the window asked for, and whether the file is handed to the
    /// chooser of the desktop once it lands.
    ///
    /// What a write answers with is the path of the file, and what is done with
    /// that path is what was asked for when the write started. A write nothing
    /// here names is some other write, and nothing happens to its file.
    saved_files: std::collections::BTreeMap<zyt_files::TaskId, bool>,
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
    /// Bytes waiting for the dialog to say which file they go into: the text
    /// of a selection, the payload of a picture.
    ///
    /// They are taken when the menu entry is chosen and not when the dialog
    /// answers: the dialog stands in place of the terminal, and a program of the
    /// session writing meanwhile is a selection that moved or went, or a
    /// picture the budget has dropped. What is saved is what was there when it
    /// was asked for.
    pub saving: Option<Vec<u8>>,
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
    /// What a run had switched off when the settings were last applied.
    applied_off: crate::scripts::Off,
    /// Every family the system has, read the first time they are offered.
    font_families: Option<Vec<crate::fonts::Family>>,
    notify: Arc<dyn Fn() + Send + Sync>,
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

        let notify: Arc<dyn Fn() + Send + Sync> = {
            let context = context.clone();
            Arc::new(move || context.request_repaint())
        };

        let mut dispatcher = KeyDispatcher::new(keymap);
        dispatcher.set_contexts(&[CONTEXT_TERMINAL]);

        let catalog = crate::themes::ThemeCatalog::load(&store);
        let mut app = Self {
            session: Session::new(notify.clone())?,
            terminal_theme: theme.terminal_theme(),
            themes: catalog,
            consoles: crate::consoles::load_all(&store),
            scripts: crate::scripts::load(&store),
            ports_memory: crate::sources::load_ports(&store),
            answered: std::collections::BTreeMap::new(),
            font: TerminalFont {
                size: settings.font_size,
                family: crate::fonts::terminal_family(),
                ..TerminalFont::default()
            },
            terminal_cache: zyt_term_egui::TerminalCache::new(),
            beside_caches: std::collections::HashMap::new(),
            theme,
            dispatcher,
            registry: build_registry(),
            menu: plate_menu::PlateMenu::new(),
            history: crate::history::Cache::default(),
            tasks: zyt_files::TaskRunner::new(),
            jobs: zyt_script::JobRunner::new(),
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
            picking: None,
            typing_reads: std::collections::BTreeSet::new(),
            saved_files: std::collections::BTreeMap::new(),
            meter: crate::metrics::Meter::new(),
            stored_clipboard: String::new(),
            lost: None,
            dropped: None,
            renaming: None,
            saving: None,
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
            applied_off: Default::default(),
            font_families: None,
            notify,
            primary: crate::primary::Primary::default(),
            hovered_link: None,
            link_menu: None,
            image_menu: None,
            store,
            settings,
        };

        app.apply_osc_settings();
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

    /// What the window is called: what it is connected to, or the program.
    pub fn title(&self) -> String {
        match &self.shown_title {
            Some(reported) if !reported.trim().is_empty() => reported.clone(),
            _ => crate::WINDOW_TITLE.to_string(),
        }
    }

    /// Names the window after what it is connected to, or after the program.
    fn apply_title(&self, context: &egui::Context) {
        context.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
    }

    /// Marks the window as wanting attention while a bell rings behind it, or
    /// a run ends behind it.
    ///
    /// A run that ends is told by its plate going away and by nothing on the
    /// desktop, so a window left alone for a long transfer asks to be looked at
    /// the way a bell asks.
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
        let rang =
            std::mem::take(&mut self.session.bell) | std::mem::take(&mut self.session.run_ended);
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
        let target = PortId::Path(path.to_string());
        let params = self.line_for(&target);
        let holds = self.holds_for(&target);

        self.session
            .connect_serial(target.clone(), path.to_string(), params, holds)?;

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

    /// What this side does with the two lines it drives on that device.
    ///
    /// A device that was never opened leaves both to the driver. There is no
    /// shared default the way there is for the line parameters: a hold is asked
    /// of one board — this one is held in reset, that one is not — and a hold
    /// carried over to whatever is plugged in next would drive a line nobody
    /// asked about.
    pub fn holds_for(&self, id: &PortId) -> zyt_serial::LineHolds {
        self.ports_memory
            .get(id)
            .map(|memory| memory.holds)
            .unwrap_or_default()
    }

    /// Which lines the device of this window shows.
    ///
    /// A session on no device answers the default set, which is what the plate
    /// and the row would draw if there were anything to draw: neither is drawn at
    /// all without a line behind it.
    pub fn shown_lines(&self) -> crate::config::ShownLines {
        self.active_port_id()
            .and_then(|id| self.ports_memory.get(&id).map(|memory| memory.shown_lines))
            .unwrap_or_default()
    }

    /// Says what to do with one of the two lines this side drives, and writes it
    /// down for the device it was asked of.
    ///
    /// It is written down for the same reason the line parameters are: a hold is
    /// a thing a device has. A board held in reset is held in reset after a
    /// disconnect and after this program is started again, which is what the
    /// worker already does across a replug.
    pub fn set_line_hold(&mut self, dtr: bool, hold: zyt_serial::LineHold) {
        let outcome = match dtr {
            true => self.session.set_dtr(hold),
            false => self.session.set_rts(hold),
        };
        self.report(outcome);

        let Some(id) = self.active_port_id() else {
            return;
        };
        let memory = self.port_memory_mut(&id);
        match dtr {
            true => memory.holds.dtr = hold,
            false => memory.holds.rts = hold,
        }
        self.save_memory(&SourceKey::Port(id));
    }

    /// The hold that keeps one of the two lines this side drives on the level
    /// it has now, or on the other one.
    ///
    /// The level is the one read back from the driver, so taking a line by hand
    /// does not move it: a line that moved when it was taken would be a pulse on
    /// the reset of a board.
    fn hold_of_level(&self, dtr: bool, flip: bool) -> zyt_serial::LineHold {
        let up = match dtr {
            true => self.session.lines.dtr_up(),
            false => self.session.lines.rts_up(),
        };
        match up != flip {
            true => zyt_serial::LineHold::Up,
            false => zyt_serial::LineHold::Down,
        }
    }

    /// Puts one of those two lines on the other level, by hand.
    ///
    /// A line the driver drives is taken by hand by the same press: the press
    /// asks for a level, and a level is what the driver does not keep.
    pub fn flip_line(&mut self, dtr: bool) {
        self.set_line_hold(dtr, self.hold_of_level(dtr, true));
    }

    /// Hands one of those two lines to the driver, or takes it by hand on the
    /// level it has now.
    ///
    /// A line already held by hand keeps the level it is held on.
    pub fn set_line_manual(&mut self, dtr: bool, manual: bool) {
        let held = match dtr {
            true => self.session.dtr_hold,
            false => self.session.rts_hold,
        };
        let hold = match manual {
            false => zyt_serial::LineHold::Auto,
            true if held.is_forced() => return,
            true => self.hold_of_level(dtr, false),
        };
        self.set_line_hold(dtr, hold);
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
        self.variables_of(self.memory_key().as_ref())
    }

    /// Writes down what a source was answered on the way in, and answers
    /// with it for the rest of this run.
    ///
    /// The file is where it outlives the run and stands in the fields the next
    /// time; the memory of it is what the connection, the menus and the
    /// settings read, so a value given here reaches everything that asks the
    /// source by name without the file being read again every frame.
    pub fn keep_answers(
        &mut self,
        key: &SourceKey,
        values: std::collections::BTreeMap<String, String>,
    ) {
        let outcome = crate::answers::save(&self.store, key, &values);
        self.answered.insert(key.clone(), values);
        self.report(outcome);
    }

    /// What one source answers by name: what its settings say, and what was
    /// typed into the window that asked before it was opened.
    ///
    /// The two are not the same thing and neither is dropped: a value of the
    /// settings is a decision about the source, and an answer typed on the way
    /// in does not overrule one. A row of the settings that stands empty is no
    /// answer, so it is filled from what was typed, the way a connection is.
    pub fn variables_of(
        &self,
        key: Option<&SourceKey>,
    ) -> std::collections::BTreeMap<String, String> {
        let Some(key) = key else {
            return std::collections::BTreeMap::new();
        };

        let mut memory = self.memory(key).cloned().unwrap_or_default();
        if let Some(answered) = self.answered.get(key) {
            crate::answers::fill(&mut memory, answered);
        }
        memory.variable_map()
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

    /// The scripts offered for the current source.
    pub fn offered_scripts(&self) -> Vec<&zyt_script::Entry> {
        self.offered_scripts_of(self.memory_key().as_ref())
    }

    /// The scripts one source is willing to offer.
    ///
    /// A source that names none is willing to offer them all, which is what
    /// every source means until somebody narrows it. What it is willing to
    /// offer and what it can actually run are two questions; this is the
    /// first, and it is what the settings ask.
    pub fn ticked_scripts_of(&self, key: Option<&SourceKey>) -> Vec<&zyt_script::Entry> {
        let named = key
            .and_then(|key| self.memory(key))
            .map(|memory| memory.scripts.clone())
            .unwrap_or_default();

        if named.is_empty() {
            return self.scripts.entries().iter().collect();
        }
        self.scripts
            .entries()
            .iter()
            .filter(|entry| named.contains(&entry.id))
            .collect()
    }

    /// The scripts one source can actually run.
    ///
    /// A script that asks this source for a value it has not got is left out:
    /// it would refuse at the moment it was started, and a list of what can be
    /// started now is the one thing a menu of them is opened for. The settings
    /// show it all the same, with a warning saying what it wants, because that
    /// is where the want is answered.
    pub fn offered_scripts_of(&self, key: Option<&SourceKey>) -> Vec<&zyt_script::Entry> {
        self.ticked_scripts_of(key)
            .into_iter()
            .filter(|entry| self.missing_variables_of(entry, key).is_empty())
            .collect()
    }

    /// The values a script asks one source for and that source has not got.
    pub fn missing_variables_of(
        &self,
        entry: &zyt_script::Entry,
        key: Option<&SourceKey>,
    ) -> Vec<String> {
        let answered = self.variables_of(key);

        entry
            .manifest
            .variables
            .iter()
            .filter(|name| !answered.contains_key(*name))
            .cloned()
            .collect()
    }

    /// Script of the current source.
    ///
    /// The one it was told to use, while that script is still there and still
    /// offered; the first of what is offered otherwise, so a source that was
    /// never asked has something to send a file with.
    pub fn active_script(&self) -> Option<&zyt_script::Entry> {
        let offered = self.offered_scripts();
        let named = self
            .memory_key()
            .and_then(|key| self.memory(&key)?.script.clone());

        named
            .and_then(|name| offered.iter().copied().find(|entry| entry.id == name))
            .or_else(|| offered.first().copied())
    }

    /// The scripts of this source that take dropped files.
    pub fn drop_scripts(&self) -> Vec<&zyt_script::Entry> {
        self.offered_scripts()
            .into_iter()
            .filter(|entry| entry.manifest.offers(Direction::Drop))
            .collect()
    }

    /// The script dropped files go to: the one in use when it takes them, the
    /// first that does otherwise.
    pub fn drop_script(&self) -> Option<&zyt_script::Entry> {
        self.active_script()
            .filter(|entry| entry.manifest.offers(Direction::Drop))
            .or_else(|| self.drop_scripts().first().copied())
    }

    /// Reads the directories of scripts again.
    pub fn reload_scripts(&mut self) {
        self.scripts = crate::scripts::load(&self.store);
    }

    /// Puts every form that was told not to ask again back to asking.
    ///
    /// A window that stopped coming up has nothing in it to press, so the way
    /// back is here and not there. How many files it changed is said in the
    /// terminal: a button that answers nothing is a button nobody believes.
    pub fn reset_forms(&mut self) {
        match crate::forms::reset(&self.store) {
            Ok(count) => self.notice(t!("settings.forms_reset_done", count = count).to_string()),
            Err(error) => self.report(Err(error)),
        }
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

    /// Throws away what is still waiting to go out to the device.
    pub fn discard_line_output(&mut self) {
        let outcome = self.session.discard_output();
        self.report(outcome);
    }

    /// Turns the hold on the reading of the port over.
    pub fn toggle_read_hold(&mut self) {
        let held = !self.session.read_hold;
        let outcome = self.session.set_read_hold(held);
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

    /// Remembers a script for the current source.
    pub fn set_transfer_script(&mut self, name: String) {
        let Some(key) = self.memory_key() else {
            return;
        };
        if let Some(memory) = self.memory_mut(&key) {
            memory.script = Some(name);
        }
        self.save_memory(&key);
    }

    /// The port the session is connected to.
    pub fn active_port_id(&self) -> Option<PortId> {
        Some(PortId::Path(self.session.port_path()?.to_string()))
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
    /// still said where every other failure is said, on the desktop, so the
    /// question says what to do and not what went wrong.
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
    pub fn osc(&self) -> crate::config::OscSettings {
        let settings = if self.session_is_trusted() {
            self.settings.osc.trusted
        } else {
            self.settings.osc.untrusted
        };
        self.session.off().apply(settings)
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
        self.beside_caches.clear();
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
        self.beside_caches.clear();
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
        self.answered.insert(key.clone(), answered);
        if let Some(directory) = console.working_directory() {
            enter_directory(&directory);
        }
        self.session.connect_console(&console)?;

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

    /// Says on the desktop that an action failed: the message the heading,
    /// its causes the body.
    ///
    /// The terminal is where the output of a device stands, and a block of
    /// this program in it would push that output up and stand in the history
    /// as if the device had said it. The same failure said again at once is
    /// said once: a port that fails every frame is one notification.
    pub fn report(&mut self, outcome: Result<()>) {
        let Err(error) = outcome else {
            return;
        };
        log::warn!("{error}: {error:?}");

        let message = t!(error.message_key()).to_string();
        let mut causes = Vec::new();
        let detail = Detail(&error).to_string();
        if !detail.is_empty() {
            causes.push(detail);
        }

        let text = format!("{message} | {}", causes.join(" | "));
        if self.last_notice.as_deref() == Some(text.as_str()) {
            return;
        }
        self.last_notice = Some(text);
        self.told_in_full(&message, &causes);
    }

    /// Says on the desktop what is true of the window itself — a file of the
    /// configuration that could not be read, a page that was put back to its
    /// defaults — as `App::told` says what an act came to.
    pub fn notice(&mut self, text: String) {
        self.last_notice = None;
        self.told(text);
    }

    /// Says on the desktop what an act of this window came to.
    ///
    /// A copy, a move, a file written, a program that ended: the act is over,
    /// its answer is one line, and it is read — if it is read at all — beside
    /// whatever the person is doing now. It goes where every other notification
    /// of this program goes, so one of them is one of them whoever asked for
    /// it.
    pub fn told(&self, text: String) {
        notify_desktop(crate::WINDOW_TITLE, &text);
    }

    /// Says on the desktop a message and what goes with it: the message is
    /// the heading, each detail a line of the body. With no details it is
    /// `App::told`.
    pub fn told_in_full(&self, message: &str, details: &[String]) {
        if details.is_empty() {
            self.told(message.to_string());
        } else {
            notify_desktop(message, &details.join("\n"));
        }
    }

    /// Forgets what the window kept of the terminal it drew, when the session
    /// shows another: a pane of tmux shown, the mode over. The picture of the
    /// grid, the link under the pointer and a search belong to that terminal.
    fn follow_the_terminal_shown(&mut self) {
        if !self.session.take_view_changed() {
            return;
        }
        self.terminal_cache.forget();
        self.hovered_link = None;
        self.search.close(&mut self.session);
    }

    /// Says on the desktop what the runs of the session came to.
    ///
    /// Whatever is said here stands between two errors, so the second of two
    /// alike is news again: a port lost, back, and lost once more says so twice.
    fn say_what_runs_came_to(&mut self) {
        for told in self.session.take_told() {
            self.last_notice = None;
            self.told_in_full(&told.message, &told.details);
        }
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
            AppCommand::PortToggleHold => self.toggle_read_hold(),
            AppCommand::PortDiscardOutput => self.discard_line_output(),
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
            AppCommand::BlockInput => self.toggle_block_input(),
            AppCommand::SelectUp => self.select_by_key(Step::Up),
            AppCommand::SelectDown => self.select_by_key(Step::Down),
            AppCommand::SelectLeft => self.select_by_key(Step::Left),
            AppCommand::SelectRight => self.select_by_key(Step::Right),
            AppCommand::SelectToLineStart => self.select_by_key(Step::LineStart),
            AppCommand::SelectToLineEnd => self.select_by_key(Step::LineEnd),
            AppCommand::Copy => self.copy_selection(),
            AppCommand::Paste => self.paste_clipboard(context),
            AppCommand::Clear => self.session.clear_screen(),
            AppCommand::ScrollPageUp => {
                let rows = self.session.page().rows as i32;
                self.session.command(Command::Scroll(rows));
            }
            AppCommand::ScrollPageDown => {
                let rows = self.session.page().rows as i32;
                self.session.command(Command::Scroll(-rows));
            }
            AppCommand::SendFile => self.transfer(Direction::Send),
            AppCommand::ReceiveFile => self.transfer(Direction::Receive),
            AppCommand::CancelTransfer => {
                let outcome = self.session.cancel_transfer();
                self.report(outcome);
            }
            AppCommand::ForceTransfer => self
                .session
                .force_transfer(crate::session::ForcedBy::Person),
            AppCommand::SearchOpen => {
                if self.ui.view == MainView::Terminal {
                    self.search.open(&mut self.session);
                    self.give_keyboard(Focus::Search);
                }
            }
            AppCommand::SearchNext => self
                .search
                .advance(&mut self.session, SearchDirection::Down),
            AppCommand::SearchPrevious => {
                self.search.advance(&mut self.session, SearchDirection::Up)
            }
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
        self.search.close(&mut self.session);
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
        let list = crate::history::List::Source(key);
        for command in commands {
            crate::history::remember(&self.store, &list, &command, &directory, false, limit);
        }
        self.history.changed(&list);
    }

    /// The commands of the current source as the file stands right now.
    ///
    /// The read goes through the cache, which asks the file whether it has been
    /// written since the last look and parses it again only then.
    pub fn command_history(&mut self) -> Vec<crate::history::Entry> {
        match self.source_commands() {
            Some(list) => self.history.commands(&self.store, &list),
            None => Vec::new(),
        }
    }

    /// The commands added by hand, which every source shares.
    pub fn added_commands(&mut self) -> Vec<crate::history::Entry> {
        self.history
            .commands(&self.store, &crate::history::List::Added)
    }

    /// The list of the current source, nothing while the window is on none.
    pub fn source_commands(&self) -> Option<crate::history::List> {
        self.memory_key().map(crate::history::List::Source)
    }

    /// Puts what is selected into the added commands.
    ///
    /// It is the selection as it stands when the entry is chosen, the way saving
    /// one into a file is: a program of the session writing meanwhile is a
    /// selection that moved or went, and what is kept is what was selected when
    /// it was asked for. The directory is the one the session stands in, the
    /// same thing a command the shell marked is written down with.
    pub fn remember_selection(&mut self) {
        self.session.ask(Purpose::Remember, Command::Copy);
    }

    /// Writes the text of the selection down in the history, once the screen
    /// answered with it.
    fn remember(&mut self, text: Option<String>) {
        let Some(text) = text.filter(|text| !text.trim().is_empty()) else {
            log::debug!("adding to the history: nothing is selected any more");
            return;
        };
        let directory = self
            .working_directory()
            .map(|directory| directory.display().to_string())
            .unwrap_or_default();
        let list = crate::history::List::Added;
        crate::history::remember(
            &self.store,
            &list,
            &text,
            &directory,
            false,
            self.settings.command_history,
        );
        self.history.changed(&list);
    }

    /// Takes one command out of the list it is kept in.
    pub fn forget_command(&mut self, list: &crate::history::List, command: &str) {
        crate::history::forget(&self.store, list, command);
        self.history.changed(list);
    }

    /// True while there are commands to offer: the ones the shell of this source
    /// marked, or the ones added by hand.
    ///
    /// The status bar asks this every frame to decide whether to draw the
    /// button at all, so it is the size of the files and not their contents.
    pub fn has_command_history(&self) -> bool {
        self.source_commands()
            .is_some_and(|list| crate::history::has_any(&self.store, &list))
            || crate::history::has_any(&self.store, &crate::history::List::Added)
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

    /// Opens the menu of tmux on the window shown: the sessions and windows as
    /// they are kept, the lists and the notice asked again. The menu never
    /// waits for the line: their replies refill it as they come.
    pub fn open_tmux_menu(&mut self) {
        let Some(menu) = self.session.tmux_menu() else {
            return;
        };
        let items = crate::ui::tmux::items(&menu);
        let opened = match crate::ui::tmux::opens_at(&menu) {
            Some(id) => self.menu.open_at(items, &id),
            None => self.menu.open(items),
        };
        if let Err(error) = opened {
            log::debug!("tmux menu: {error}");
            return;
        }
        self.menu.notice(crate::ui::tmux::notice(&menu));
        self.session.tmux_refresh();
    }

    /// Refills the menu of tmux while it stands, when the lists came back:
    /// the query, the level and the selection stay. While the reply of the
    /// notice is on its way, what tmux said last stands.
    fn refill_tmux_menu(&mut self) {
        if !self.session.take_tmux_lists_changed() || !self.menu.holds(crate::ui::tmux::DETACH) {
            return;
        }
        let Some(menu) = self.session.tmux_menu() else {
            return;
        };
        if let Err(error) = self.menu.refill(crate::ui::tmux::items(&menu)) {
            log::debug!("tmux menu: {error}");
        }
        self.menu.notice(crate::ui::tmux::notice(&menu));
    }

    /// Opens the commands to type back: the ones the shell of this source marked
    /// and the ones added by hand.
    ///
    /// A history nothing has written to yet opens nothing and says nothing: a
    /// console that reports no commands is the ordinary case, and a message
    /// about it would stand in the output of the console it is about.
    pub fn open_history_menu(&mut self) {
        let items = crate::ui::history::items(self);
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
    /// `caret` is `Entry::caret`, what the switch of the plate was when the
    /// command was sent: an entry that ran its carets as control codes runs
    /// them as control codes again. A command is kept as the text it was
    /// written in, so that flag is the only thing that says what the text means.
    ///
    /// Such an entry gets no return of its own — `closes_a_command` — because
    /// the notation already says where it ends: a `^M` written at the end is the
    /// key that runs it, and one added after a command that carries none is a
    /// line nobody typed.
    ///
    /// Nothing is written down either way — an entry says when it last ran, and
    /// a command that was typed back has not run yet. The shell says when it
    /// does, and that is what moves it to the top.
    pub fn run_from_history(&mut self, command: String, caret: bool, action: HistoryAction) {
        let mut bytes = typed_again(&command, caret);
        if closes_a_command(action, caret) {
            bytes.push(b'\r');
        }
        self.send_input(&bytes);
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
        self.search.apply(&mut self.session);
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

    /// Grows the selection of the keyboard one step.
    ///
    /// The view stops following the output while a selection is being made with
    /// the keys, the way it stops while one is dragged: the caret is what the
    /// page follows then, and output arriving would pull the page away from
    /// what is being selected.
    fn select_by_key(&mut self, step: Step) {
        self.session.command(Command::SelectStep(step));
    }

    /// Opens the window of a command of several lines, or closes it.
    ///
    /// What is in the field is kept while it stands and dropped when it goes:
    /// the window is the command, so closing it is deciding not to send one.
    ///
    /// The switch of the carets is read from the source as it opens, which is
    /// the one moment it has to be: the window may have moved to another source
    /// since the plate last stood, and a switch standing where the last source
    /// left it is a block sent as nobody asked.
    pub fn toggle_block_input(&mut self) {
        self.ui.block = match self.ui.block {
            Some(_) => None,
            None => {
                self.ui.block_caret = self.remembered_block_caret();
                Some(String::new())
            }
        };
        self.ui.block_drawn = false;
    }

    /// Whether this source reads the carets of a block as control codes.
    ///
    /// A source that was never asked reads them as the characters they are: a
    /// caret is a character of a shell before it is a notation.
    fn remembered_block_caret(&self) -> bool {
        self.memory_key()
            .and_then(|key| self.memory(&key).map(|memory| memory.block_caret))
            .unwrap_or_default()
    }

    /// Turns the switch of the carets and writes it down for this source.
    ///
    /// It is written the moment it is turned and not when the block is sent: a
    /// plate closed with `Esc` sends nothing, and the switch of it was still an
    /// answer about this source. A window on no source turns it for as long as
    /// the plate stands, because there is no file to write it to.
    pub fn set_block_caret(&mut self, caret: bool) {
        self.ui.block_caret = caret;
        let Some(key) = self.memory_key() else {
            return;
        };
        let written = match self.memory_mut(&key) {
            Some(memory) if memory.block_caret != caret => {
                memory.block_caret = caret;
                true
            }
            _ => false,
        };
        if written {
            self.save_memory(&key);
        }
    }

    /// Types what the window holds into the source, writes it down and closes.
    ///
    /// Every line is closed with a carriage return, the way every other line
    /// this application types into a device is, and the last one too: a block
    /// whose last line waited for a key would be a block that did not run.
    ///
    /// The whole of it is one entry of the history of this source — not of the
    /// commands added by hand, which are a list somebody keeps — because this
    /// is a command that ran here, and the one thing wanted of it later is to
    /// run it again. The switch is written down with it: what is kept is the
    /// text with the carets still in it, so the entry has to say whether they
    /// were bytes when it ran.
    pub fn send_block_input(&mut self) {
        let Some(text) = self.ui.block.take() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }

        self.send_input(&typed_block(&text, self.ui.block_caret));

        let Some(key) = self.memory_key() else {
            return;
        };
        let directory = self
            .working_directory()
            .map(|directory| directory.display().to_string())
            .unwrap_or_default();
        let list = crate::history::List::Source(key);
        crate::history::remember(
            &self.store,
            &list,
            &text,
            &directory,
            self.ui.block_caret,
            self.settings.command_history,
        );
        self.history.changed(&list);
    }

    /// Lets a selection go, whichever way it was made.
    ///
    /// It is the one way out of the mode, so every way in asks for this one:
    /// `Esc`, input sent to the device, a press on the plate of the counts and
    /// the sign in the status bar. The plate forgets its corner by itself once
    /// the selection is gone.
    pub fn leave_selection(&mut self) {
        self.session.command(Command::SelectClear);
    }

    /// Sends what the person typed, pasted or picked to the device.
    ///
    /// Input is what ends a selection: somebody who sends something is done
    /// looking at it, and a selection kept would hold the source, so the echo
    /// of that input would not arrive. A press that sends nothing — a modifier
    /// alone, a key a binding claims — leaves it standing. The page goes to the
    /// bottom, where the answer will be, and nothing reaches the device while a
    /// transfer holds the line, because its protocol owns every byte of it.
    ///
    /// Answers of the terminal and the bytes of a script do not come this way:
    /// they are not input, and they end no selection.
    pub fn send_input(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.leave_selection();
        self.session.command(Command::ScrollToBottom);
        if !self.session.is_transferring() {
            self.session.write(bytes);
        }
    }

    /// Sends a key tmux knows by `name`, with `bytes` for where it does not:
    /// what `send_input` does for bytes.
    fn send_key(&mut self, name: &str, bytes: &[u8]) {
        self.leave_selection();
        self.session.command(Command::ScrollToBottom);
        if !self.session.is_transferring() {
            self.session.write_key(name, bytes);
        }
    }

    /// The bytes a paste of `text` is, in the modes the terminal is in now.
    fn pasted(&mut self, text: &str) -> Vec<u8> {
        zyt_term::encode_paste(text, self.session.page().modes)
    }

    fn copy_selection(&mut self) {
        self.session.ask(Purpose::Copy, Command::Copy);
    }

    /// Pastes what the middle button pastes on this platform: the selection of
    /// the platform, the selection of this terminal, or the clipboard.
    ///
    /// The selection of this terminal is read before it is sent, because
    /// sending it is what lets it go.
    pub fn paste_selection_or_clipboard(&mut self) {
        match self.primary.text() {
            Some(text) => {
                let pasted = self.pasted(&text);
                self.send_input(&pasted);
            }
            None => self.session.ask(Purpose::Paste, Command::Copy),
        }
    }

    /// The selection of this terminal came back for a paste: it is sent, or
    /// the clipboard is when there is none.
    fn paste_selected(&mut self, text: Option<String>, context: &egui::Context) {
        match text.filter(|text| !text.is_empty()) {
            Some(text) => {
                let pasted = self.pasted(&text);
                self.send_input(&pasted);
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

    /// Asks the dialog which file the selected text of the terminal goes into.
    ///
    /// Nothing is offered without a selection, so there is nothing to answer for
    /// one that is empty: the menu entry is only there while there is text, and a
    /// selection of spaces alone is not text anybody asked to keep.
    ///
    /// The directory it opens in is the one the session stands in when the shell
    /// says, because a selection is almost always about the work going on there;
    /// the dialog remembers where it was left otherwise.
    pub fn save_selection(&mut self, open_with: bool) {
        self.session.ask(Purpose::Save { open_with }, Command::Copy);
    }

    /// Asks the dialog where the text of the selection goes, once the screen
    /// answered with it.
    fn save_selected(&mut self, text: Option<String>, open_with: bool) {
        let Some(text) = text.filter(|text| !text.trim().is_empty()) else {
            log::debug!("saving the selection: nothing is selected any more");
            return;
        };

        self.read_dialog_memory();
        if let Some(directory) = self.working_directory() {
            self.file_dialog.config_mut().initial_directory = directory;
        }
        self.file_dialog.config_mut().default_file_name = SELECTION_FILE.to_string();
        self.file_dialog.config_mut().title = Some(t!("selection.save_title").to_string());
        self.file_dialog.config_mut().title_bar = true;

        self.saving = Some(text.into_bytes());
        self.pending_pick = Some(PendingPick::SaveFile { open_with });
        self.file_dialog.save_file();
        self.show_view(MainView::FileDialog);
    }

    /// Writes what was waiting into the file the dialog named.
    ///
    /// The write is a task like any other file work, so a selection of a whole
    /// scrollback does not hold the window still while it lands, and the panel of
    /// what is running shows it and can stop it.
    fn write_saved(&mut self, path: &std::path::Path, open_with: bool) {
        let Some(bytes) = self.saving.take() else {
            return;
        };
        let id = self.tasks.start(
            zyt_files::FileTask::Write {
                path: path.to_path_buf(),
                bytes,
            },
            Some(self.notify.clone()),
        );
        self.saved_files.insert(id, open_with);
    }

    /// Puts a picture a program sent where the dialog says, as a PNG.
    ///
    /// The bytes are encoded now and not when the dialog answers: the dialog
    /// stands open for as long as somebody is looking for a folder, and nothing
    /// of a picture is promised to outlive that.
    pub fn save_picture(&mut self, pixels: &zyt_term::Pixels, open_with: bool) {
        let bytes = match encode_png(pixels) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::warn!("saving a picture: {error}");
                self.told(t!("image.not_saved").to_string());
                return;
            }
        };

        self.read_dialog_memory();
        if let Some(directory) = self.working_directory() {
            self.file_dialog.config_mut().initial_directory = directory;
        }
        self.file_dialog.config_mut().default_file_name = PICTURE_FILE.to_string();
        self.file_dialog.config_mut().title = Some(t!("image.save_title").to_string());
        self.file_dialog.config_mut().title_bar = true;

        self.saving = Some(bytes);
        self.pending_pick = Some(PendingPick::SaveFile { open_with });
        self.file_dialog.save_file();
        self.show_view(MainView::FileDialog);
    }

    /// Puts a picture on the clipboard of the platform.
    ///
    /// It goes as a picture and not as the payload it was decoded from: what
    /// takes a sixel payload out of a clipboard is nothing, and every program
    /// that takes a picture takes this.
    pub fn copy_picture(&mut self, pixels: &zyt_term::Pixels) {
        match self.copy_rgba(pixels.width, pixels.height, pixels.rgba.clone()) {
            Ok(()) => self.told(
                t!(
                    "image.copied",
                    size = format!("{}x{}", pixels.width, pixels.height),
                    volume = crate::format::size(pixels.rgba.len() as u64)
                )
                .to_string(),
            ),
            Err(error) => {
                log::warn!("clipboard picture: {error}");
                self.told(t!("image.not_copied").to_string())
            }
        }
    }

    /// Folds a picture into the line it left on the page, or unfolds it into
    /// the picture again.
    ///
    /// What the menu holds is the picture as it was when the menu opened, so it
    /// is taken again afterwards: the entry says fold or unfold by the state it
    /// is looking at, and the menu may be opened on the same picture twice.
    pub fn fold_picture(&mut self, id: zyt_term::ImageId, folded: bool) {
        self.session.command(Command::FoldPicture { id, folded });
        self.session
            .ask(Purpose::PictureMenu(id), Command::Picture(id));
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
            self.told(t!("file.not_text_for_terminal", media = content.media_type()).to_string());
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
    pub fn cancel_job(&mut self, id: zyt_script::JobId) {
        let outcome = self.jobs.cancel(id).map_err(AppError::from);
        self.report(outcome);
    }

    /// Takes one entry of that list away, with the file of its output.
    pub fn forget_job(&mut self, id: zyt_script::JobId) {
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
                Some(zyt_script::Outcome::Done) => {
                    t!("job.finished", command = command, took = took)
                }
                Some(zyt_script::Outcome::Failed(Some(code))) => {
                    t!("job.failed", command = command, code = code, took = took)
                }
                _ => t!("job.stopped", command = command, took = took),
            };
            self.told(text.to_string());
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
                    self.told(t!("file.moved", path = path.display().to_string()).to_string())
                }
                Ok(zyt_files::Done::Trashed(path)) => {
                    self.told(t!("file.trashed", path = path.display().to_string()).to_string())
                }
                Ok(zyt_files::Done::Deleted(path)) => {
                    self.told(t!("file.deleted", path = path.display().to_string()).to_string())
                }
                Ok(zyt_files::Done::Read { path, bytes }) => {
                    if self.typing_reads.remove(&id) {
                        self.type_contents(&path, bytes);
                    } else {
                        self.copy_to_clipboard(&path, bytes, context);
                    }
                }
                Ok(zyt_files::Done::Written(path)) => {
                    if let Some(open_with) = self.saved_files.remove(&id) {
                        self.told(
                            t!(
                                "file.written",
                                path = path.display().to_string(),
                                volume = crate::format::size(file_size(&path))
                            )
                            .to_string(),
                        );
                        if self.settings.copy_saved_path {
                            context.copy_text(path.display().to_string());
                        }
                        if open_with {
                            self.open_file_with(&path);
                        }
                    }
                }
                Err(zyt_files::FileError::Cancelled) => self.told(t!("file.cancelled").to_string()),
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
        let copied = |volume: usize| {
            t!(
                "file.copied",
                path = path.display().to_string(),
                media = content.media_type(),
                volume = crate::format::size(volume as u64)
            )
            .to_string()
        };

        match &content {
            zyt_files::Content::Text(_) => match String::from_utf8(bytes) {
                Ok(text) => {
                    let volume = text.len();
                    context.copy_text(text);
                    self.told(copied(volume));
                }
                Err(_) => self.told(t!("file.not_text", media = content.media_type()).to_string()),
            },
            zyt_files::Content::Image(_) => {
                let volume = bytes.len();
                match self.copy_image(&bytes) {
                    Ok(()) => self.told(copied(volume)),
                    Err(error) => {
                        log::warn!("clipboard image: {error}");
                        self.told(t!("file.not_copied", media = content.media_type()).to_string())
                    }
                }
            }
            zyt_files::Content::Other(_) => {
                self.told(t!("file.not_copied", media = content.media_type()).to_string())
            }
        }
    }

    /// Puts a picture of a file into the clipboard of the platform.
    fn copy_image(&mut self, bytes: &[u8]) -> std::result::Result<(), String> {
        let image = image::load_from_memory(bytes)
            .map_err(|error| error.to_string())?
            .into_rgba8();
        let (width, height) = image.dimensions();

        self.copy_rgba(width as usize, height as usize, image.into_raw())
    }

    /// Puts pixels into the clipboard of the platform.
    ///
    /// Every picture this window copies goes through here, whichever it came
    /// from: a file the menu read, a picture a program drew.
    fn copy_rgba(
        &mut self,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
    ) -> std::result::Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
        clipboard
            .set_image(arboard::ImageData {
                width,
                height,
                bytes: std::borrow::Cow::Owned(rgba),
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
        let paths: Vec<std::path::PathBuf> = dropped
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect();
        if paths.is_empty() {
            return;
        }

        if !self.session.has_source() {
            self.notice(t!("drop.not_connected").to_string());
            return;
        }

        self.dropped = Some(paths);
        self.open_drop_menu();
    }

    /// Puts up the dialog a script is waiting on, while one is waiting.
    ///
    /// One stands at a time: a script asks a question and has nothing to do
    /// until it is answered, so there is never a second one behind it.
    ///
    /// What this source answered this form of this script last time is read
    /// first. Unless something says to ask — the switch of that form, a form
    /// that is not kept at all, or a field the saved answers leave unanswered —
    /// the question is answered from the file and no window is drawn at all: a
    /// second file to the same board is one press.
    fn show_asked_dialog(&mut self) {
        self.drop_stale_pick();
        if self.ui.form.is_some() || self.picking.is_some() {
            return;
        }
        let pending = match self.session.take_ask() {
            Some(crate::scripts::Waiting::Form(pending)) => pending,
            Some(crate::scripts::Waiting::Pick(picking)) => {
                self.open_script_pick(picking);
                return;
            }
            None => return,
        };

        let key = self.memory_key();
        let script = self
            .session
            .script_id()
            .map(str::to_string)
            .unwrap_or_default();
        let form = match pending.question.id.is_empty() {
            true => crate::forms::DEFAULT_FORM.to_string(),
            false => pending.question.id.clone(),
        };
        let saved = key
            .as_ref()
            .filter(|_| !script.is_empty() && !pending.question.unsaved)
            .and_then(|key| crate::forms::load(&self.store, key, &script, &form));

        if let Some(values) = crate::forms::reuse(saved.as_ref(), &pending.question) {
            log::info!("{script} runs with what {form} was answered before");
            pending.answer(Some(values));
            return;
        }

        let always_ask = saved
            .as_ref()
            .map(crate::forms::Answered::always_ask)
            .unwrap_or(true);

        self.ui.form = Some(crate::ui::form::State::new(
            pending, key, script, form, saved, always_ask,
        ));
    }

    /// Offers the entries of one list of a dialog a script is asking.
    ///
    /// A list of a dialog is a menu of this program like any other, so it is
    /// found by typing and looks like the rest; what it changes is the field
    /// of the window standing behind it.
    pub fn open_form_menu(&mut self, field: &str) {
        let Some(state) = self.ui.form.as_ref() else {
            return;
        };
        let items = crate::ui::menu::form_items(state, field);
        let current = crate::ui::menu::form_entry(state, field);
        if let Err(error) = self.menu.open_at(items, &current) {
            log::debug!("the list of a dialog: {error}");
        }
    }

    /// Opens one directory with whatever the desktop opens directories with.
    pub fn open_directory(&mut self, directory: &std::path::Path) {
        let outcome = open::that_detached(directory).map_err(|source| AppError::Link { source });
        self.report(outcome);
    }

    /// Opens the directory a script of one's own belongs in.
    ///
    /// The configuration directory, because that is the one of the four a
    /// person writes in: it is searched last, so a script there stands in for
    /// one of the same name shipped with the program.
    pub fn open_scripts_directory(&mut self) {
        let directory = self.store.config_dir().join(zyt_script::SCRIPTS);
        if let Err(error) = std::fs::create_dir_all(&directory) {
            log::warn!("cannot make the directory of the scripts: {error}");
        }
        let outcome = open::that_detached(&directory).map_err(|source| AppError::Link { source });
        self.report(outcome);
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
        let quoted = zyt_script::quote_for_shell(&path.to_string_lossy());
        self.type_text(&quoted);
    }

    /// Types text into the session the way a paste is typed.
    pub fn type_text(&mut self, text: &str) {
        let pasted = self.pasted(text);
        self.send_input(&pasted);
    }

    /// The paths of the files that were dropped, taken out of the wait.
    pub fn take_dropped(&mut self) -> Option<Vec<std::path::PathBuf>> {
        self.dropped.take()
    }

    /// Pastes the clipboard into the terminal.
    ///
    /// The toolkit owns the clipboard, so the request goes to it and comes back
    /// as a paste event, the same path a key press takes.
    fn paste_clipboard(&mut self, context: &egui::Context) {
        self.ui.paste_pending = true;
        context.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }

    fn transfer(&mut self, direction: Direction) {
        let Some(script) = self.active_script().cloned() else {
            self.report(Err(AppError::NoScript));
            return;
        };
        if !script.manifest.offers(direction) {
            self.report(Err(AppError::NoScript));
            return;
        }

        self.begin_script(&script, direction, Target::None);
    }

    /// Hands files dropped onto the window to the script that takes them.
    pub fn start_dropped(&mut self, paths: &[std::path::PathBuf]) {
        let Some(script) = self.drop_script().cloned() else {
            self.report(Err(AppError::NoScript));
            return;
        };
        if paths.is_empty() {
            return;
        }
        self.begin_script(&script, Direction::Drop, Target::Files(paths));
    }

    /// Puts up the dialog of the file system a running script asked for.
    ///
    /// It opens where this script on this source picked last time, which is
    /// where a second file to the same board is.
    fn open_script_pick(&mut self, picking: crate::scripts::Picking) {
        use zyt_script::PickKind;

        self.read_dialog_memory();
        let pick = picking.question.clone();
        let title = match pick.title.is_empty() {
            false => pick.title.clone(),
            true if pick.kind.is_directory() => t!("menu.pick_directory_title").to_string(),
            true => t!("menu.pick_file_title").to_string(),
        };
        let remembered = self.remembered_directory();
        let config = self.file_dialog.config_mut();
        config.title = Some(title);
        config.title_bar = true;
        config.default_file_name = pick.name.clone().unwrap_or_default();
        if let Some(directory) = remembered {
            config.initial_directory = directory;
        }

        match pick.kind {
            PickKind::File => self.file_dialog.pick_file(),
            PickKind::Files | PickKind::Directories => self.file_dialog.pick_multiple(),
            PickKind::Directory => self.file_dialog.pick_directory(),
            PickKind::Save => self.file_dialog.save_file(),
        }
        self.pending_pick = Some(PendingPick::Script);
        self.picking = Some(picking);
        self.show_view(MainView::FileDialog);
    }

    /// Answers the dialog of the file system a script waits on.
    ///
    /// What was picked is remembered for this script on this source: the
    /// directory picked, or the one the first file stands in.
    fn answer_pick(&mut self, paths: Option<Vec<std::path::PathBuf>>) {
        let Some(picking) = self.picking.take() else {
            return;
        };
        if let Some(first) = paths.as_ref().and_then(|paths| paths.first()) {
            let directory = match first.is_dir() {
                true => Some(first.clone()),
                false => first.parent().map(std::path::Path::to_path_buf),
            };
            self.remember_directory(directory);
        }
        picking.answer(paths);
    }

    /// Puts away a dialog of the file system whose run is gone: a run stopped
    /// or cut off while it stood is not there to be told.
    fn drop_stale_pick(&mut self) {
        let stale = self
            .picking
            .as_ref()
            .and_then(crate::scripts::Asking::run)
            .is_some_and(|run| self.session.run_number() != Some(run));
        if !stale {
            return;
        }
        if let Some(picking) = self.picking.take() {
            picking.answer(None);
        }
        if self.pending_pick == Some(PendingPick::Script) {
            // The dialog has no way to be shut from outside; the view it stands
            // in is left, and the next opening starts it afresh.
            self.pending_pick = None;
            self.show_view(self.return_view);
        }
    }

    /// Where the running script picked last time on this source.
    fn remembered_directory(&self) -> Option<std::path::PathBuf> {
        let key = self.memory_key()?;
        let script = self.session.script_id()?;
        self.memory(&key)?
            .pick_directories
            .get(script)
            .cloned()
            .filter(|path| path.is_dir())
    }

    /// Writes down where the running script picked, for the next time.
    fn remember_directory(&mut self, directory: Option<std::path::PathBuf>) {
        let (Some(directory), Some(key)) = (directory, self.memory_key()) else {
            return;
        };
        let Some(script) = self.session.script_id().map(str::to_string) else {
            return;
        };
        if let Some(memory) = self.memory_mut(&key) {
            memory.pick_directories.insert(script, directory);
        }
        self.save_memory(&key);
    }

    /// Asks the dialog for one path, for whatever is waiting for one.
    ///
    /// The dialog says which of the two it is asking for, because the same
    /// window picks a file for a transfer and this.
    fn pick_one_path(&mut self, pending: PendingPick, directory: bool) {
        self.read_dialog_memory();
        let title = match (&pending, directory) {
            (PendingPick::InsertContents, _) => t!("menu.insert_file_title"),
            (_, true) => t!("menu.pick_directory_title"),
            (_, false) => t!("menu.pick_file_title"),
        };
        self.file_dialog.config_mut().title = Some(title.to_string());
        self.file_dialog.config_mut().title_bar = true;

        self.pending_pick = Some(pending);
        if directory {
            self.file_dialog.pick_directory();
        } else {
            self.file_dialog.pick_file();
        }
        self.show_view(MainView::FileDialog);
    }

    /// Asks the dialog for a path and types it into the session.
    pub fn pick_path_to_type(&mut self, directory: bool) {
        self.pick_one_path(PendingPick::TypePath, directory);
    }

    /// Asks the dialog for a file and types what it holds into the session.
    pub fn pick_file_to_insert(&mut self) {
        self.pick_one_path(PendingPick::InsertContents, false);
    }

    /// Asks the dialog for the path one row of a script's window stands on.
    ///
    /// The window of the script is not drawn while the dialog stands, and what
    /// has been answered in it so far is waiting in `ui.form`: the path goes
    /// into that row and the window comes back with it, which is what makes a
    /// path one answer of a form rather than something asked for before the
    /// script runs.
    pub fn pick_form_path(&mut self, field: &str, directory: bool) {
        self.pick_one_path(PendingPick::FormPath(field.to_string()), directory);
    }

    /// Writes a picked path into the row of a script's window that asked for
    /// it.
    pub fn set_form_path(&mut self, field: &str, path: &std::path::Path) {
        let Some(form) = self.ui.form.as_mut() else {
            return;
        };
        form.values.insert(
            field.to_string(),
            zyt_script::Value::Text(path.display().to_string()),
        );
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
        let Some(pending) = self.pending_pick.clone() else {
            return;
        };

        if let Some(paths) = self.file_dialog.take_picked_multiple() {
            let return_view = self.return_view;
            self.save_file_dialog();
            if let PendingPick::Script = pending {
                self.answer_pick(Some(paths));
            }
            self.show_view(return_view);
            return;
        }

        if let Some(path) = self.file_dialog.take_picked() {
            let return_view = self.return_view;
            self.save_file_dialog();
            match pending {
                PendingPick::Script => self.answer_pick(Some(vec![path])),
                PendingPick::Rename => {
                    if let Some(from) = self.renaming.take() {
                        self.start_file_task(zyt_files::FileTask::Move { from, to: path });
                    }
                }
                PendingPick::TypePath => self.type_path(&path),
                PendingPick::InsertContents => self.insert_file_contents(&path),
                PendingPick::SaveFile { open_with } => self.write_saved(&path, open_with),
                PendingPick::ConsoleDirectory(index) => self.set_console_directory(index, &path),
                PendingPick::FormPath(field) => self.set_form_path(&field, &path),
            }
            self.show_view(return_view);
            return;
        }

        if !matches!(
            self.file_dialog.state(),
            egui_file_dialog::DialogState::Open
        ) {
            // A dialog that was closed with nothing picked leaves nothing
            // waiting: the text of a selection nobody saved would otherwise sit
            // in the window until the next one was asked for, and a script
            // waiting on it is told nothing was picked.
            self.saving = None;
            if pending == PendingPick::Script {
                self.answer_pick(None);
            }
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

    /// Starts a script the way its manifest asks to be started.
    ///
    /// A script that holds the line goes to the session, which has the line to
    /// give it and lets one of them run at a time. One that does not hold the
    /// line runs the same way and is given a line that is already shut: what
    /// it does reaches the device through nothing but the programs it starts.
    fn begin_script(
        &mut self,
        entry: &zyt_script::Entry,
        direction: Direction,
        target: Target<'_>,
    ) {
        let variables = self.source_variables();
        let roots = crate::scripts::roots(&self.store);
        let outcome = self.session.start_script(
            entry,
            &roots,
            direction,
            Targets::from(target),
            &variables,
            self.notify.clone(),
        );
        self.report(outcome);
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
        let progress = self.session.outside_progress();
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

        if !self.session.page().modes.mouse_report {
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
            notify_desktop(&heading, &notification.body);
        }

        // What the far side says about its directory is only acted on where the
        // settings of this session allow it: the answer moves the working
        // directory of this process, and the file dialog, a dropped file and a
        // new window all read it afterwards. What is refused is dropped rather
        // than kept, the way every other refused sequence is.
        let said = self.session.working_directory.take();
        if let Some(directory) = said.filter(|_| self.osc().directory) {
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
        self.told(t!("console.restarting", name = shown).to_string());
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
    /// Tells the terminal the size a larger picture is shrunk to, what one
    /// picture may weigh and what the pictures may hold together.
    ///
    /// The size may be a share of the window, so it is worked out again every
    /// frame: a window resized or a font changed is a different number of
    /// pixels on the page, and a program that asks what it may send has to be
    /// told what is true now. It is a multiplication and a call, and the call
    /// only drops pictures when the budget is the thing that changed.
    fn apply_picture_limits(&mut self, context: &egui::Context) {
        let window = self.terminal_pixels(context);
        let limits = self.settings.sixel;
        self.session.set_picture_limits(
            limits.limit(window),
            limits.weight_bytes(),
            limits.budget(),
        );
    }

    /// The window as a picture is measured against it: its pixels, as the page
    /// stands now.
    ///
    /// The settings page reads it to say what a share comes to and whether the
    /// two numbers of the block agree.
    pub fn terminal_pixels(&self, context: &egui::Context) -> (usize, usize) {
        let cell = self.font.cell_size(context) * context.pixels_per_point();
        let (columns, rows) = (self.session.content().columns, self.session.content().rows);
        (
            (columns as f32 * cell.x).round().max(0.0) as usize,
            (rows as f32 * cell.y).round().max(0.0) as usize,
        )
    }

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
        let pictures = self.osc().sixel.access();
        self.session.set_picture_access(pictures);
        self.session.set_tmux_control(self.osc().tmux_control);
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
            self.session.command(Command::AnswerClipboard(Some(stored)));
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
                self.session
                    .command(Command::AnswerClipboard(Some(String::new())));
            }
            context.request_repaint();
            return;
        }

        self.ui.clipboard_stage = ClipboardStage::Waiting;
        self.ui.clipboard_asked = Some(Instant::now());
        context.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        context.request_repaint();
    }

    /// Does what each answer of the screen was asked for.
    fn act_on_replies(&mut self, context: &egui::Context) {
        use zyt_term::Answer;
        for (purpose, answer) in self.session.take_replies() {
            match (purpose, answer) {
                (Purpose::Publish, Answer::Copied(Some(text))) if !text.is_empty() => {
                    self.publish_selection(&text, context);
                }
                (Purpose::Copy, Answer::Copied(Some(text))) => context.copy_text(text),
                (Purpose::Paste, Answer::Copied(text)) => self.paste_selected(text, context),
                (Purpose::Save { open_with }, Answer::Copied(text)) => {
                    self.save_selected(text, open_with);
                }
                (Purpose::Remember, Answer::Copied(text)) => self.remember(text),
                (Purpose::SearchQuery, Answer::Copied(text)) => {
                    self.search
                        .query_answered(&mut self.session, text.as_deref());
                }
                (Purpose::SearchStep, Answer::Found(found)) => self.search.answered(found),
                (Purpose::PictureMenu(id), Answer::Picture(picture)) => {
                    let menu = picture.map(|picture| PictureMenu { id, picture });
                    crate::ui::terminal::open_picture_menu(self, menu);
                }
                _ => {}
            }
        }
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

    /// Asks the desktop which program should open a file this window wrote.
    ///
    /// It does not ask what a link of the session has to answer for: that address
    /// is an untrusted session's word for a file of this machine, and this is a
    /// file this window has just written, at a path somebody picked in a dialog.
    pub fn open_file_with(&mut self, path: &std::path::Path) {
        let outcome = crate::openwith::open_with(&path.to_string_lossy())
            .map_err(|source| AppError::Link { source });
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
        self.told(t!("link.not_opened").to_string());
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
        if self.session.screen_turning() {
            context.request_repaint_after(Duration::from_millis(500));
        }

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
            self.session.command(Command::ForgetScrollback);
            self.source_menu_wanted = true;
        }
        self.had_source = has_source;
    }

    /// Tells the grid how many lines its budget buys at the width it has now.
    ///
    /// The budget is memory and a row costs its full width, so the number of
    /// lines follows the window: widening it drops the oldest lines rather than
    /// quietly costing more, and narrowing it keeps more of them. It is asked on
    /// every frame because every frame is where the width may have changed, and
    /// the grid does nothing at all when the number it is told is the one it has.
    pub fn apply_scrollback(&mut self) {
        self.session
            .set_scrollback_memory(self.settings.scrollback_memory);
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
    /// Every source carries the moment it was last opened on the plate of its
    /// whole, and the menu makes room for the plate of the selection alone, so
    /// its place and its length follow what that plate holds. The ports the
    /// process may not open are counted in the line above the entries, because
    /// a device that is there and cannot be opened is not a device that is
    /// missing.
    pub fn open_source_menu(&mut self) {
        self.refresh_sources();
        let items = crate::ui::connect::items(self);
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
    ///
    /// What the keys and the pastes of a frame send goes out in one write,
    /// through `send_input`, and that is what lets a selection go: a key that
    /// sends nothing — a modifier alone, a combination a binding claims — leaves
    /// it standing. `Esc` is the one key that ends the mode without reaching
    /// the device, because it is what every other window of this program is
    /// left with.
    fn handle_keyboard(&mut self, context: &egui::Context) {
        self.settle_contexts();
        if self.ui.form.is_some()
            || self.menu.is_open()
            || self.ui.ask.is_some()
            || self.ui.tmux_rename.is_some()
            || self.ui.block.is_some()
        {
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
                    if self.ui.focus != Focus::Terminal {
                        continue;
                    }
                    if key == egui::Key::Escape && self.session.page().selecting {
                        self.leave_selection();
                        continue;
                    }
                    self.encode_terminal_key(key, &modifiers, &mut bytes);
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
                    self.session.command(Command::AnswerClipboard(Some(text)));
                }
                egui::Event::Paste(text) if self.ui.paste_pending => {
                    self.ui.paste_pending = false;
                    bytes.extend(self.pasted(&text));
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
        self.send_input(&bytes);
    }

    /// Handles the clipboard shortcuts.
    ///
    /// The window toolkit turns them into clipboard events and swallows the key
    /// press, so the press is rebuilt here and offered to the key bindings like
    /// any other combination. What no binding claims reaches the terminal.
    ///
    /// A keyboard with keys of its own for the clipboard makes the same events
    /// with nothing held, and those are not a shortcut: they are handled by
    /// `clipboard_key` instead.
    fn clipboard_shortcut(
        &mut self,
        key: char,
        pasted: Option<&str>,
        context: &egui::Context,
        commands: &mut Vec<CommandId>,
        bytes: &mut Vec<u8>,
    ) {
        let modifiers = context.input(|input| input.modifiers);
        if !clipboard_shortcut_held(&modifiers) {
            self.clipboard_key(pasted, bytes);
            return;
        }
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
                    bytes.extend(self.pasted(text));
                } else {
                    commands.push(id);
                }
            }
            Dispatch::Pending => {}
            Dispatch::Unhandled => {
                if self.ui.focus != Focus::Terminal {
                    return;
                }
                let modes = self.session.page().modes;
                let encoded = zyt_term::encode_key(
                    &zyt_term::Key::Char(key),
                    zyt_term::Modifiers {
                        shift: modifiers.shift,
                        ctrl: true,
                        alt: modifiers.alt,
                    },
                    modes,
                    self.settings.shift_enter.kind(),
                );
                if let Some(encoded) = encoded {
                    bytes.extend(encoded);
                }
            }
        }
    }

    /// What a key of its own for the clipboard does.
    ///
    /// The key says what it is for, so it does that and nothing else: the
    /// selection goes to the clipboard, the clipboard goes to the device as a
    /// paste. No control byte is sent — `^C`, `^X` and `^V` are what the
    /// shortcuts of those letters mean, and a key that names the operation
    /// never asked for a letter.
    ///
    /// *Cut* copies: the grid is what a device printed, so there is nothing to
    /// take out of it.
    ///
    /// A field of the interface answers these events itself, so nothing is done
    /// while the terminal has not got the keyboard.
    fn clipboard_key(&mut self, pasted: Option<&str>, bytes: &mut Vec<u8>) {
        if self.ui.focus != Focus::Terminal {
            return;
        }
        match pasted {
            Some(text) => bytes.extend(self.pasted(text)),
            None => self.copy_selection(),
        }
    }

    /// Encodes a key for the terminal, into `bytes`.
    ///
    /// In tmux control mode a key tmux knows by name goes by that name, since
    /// tmux encodes it for the terminal of the pane: what was typed before it
    /// is sent first, so the order stays.
    fn encode_terminal_key(
        &mut self,
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
        let modes = self.session.page().modes;
        let Some(encoded) =
            zyt_term::encode_key(&mapped, mods, modes, self.settings.shift_enter.kind())
        else {
            return;
        };
        let named = self
            .session
            .tmux_takes_names()
            .then(|| crate::tmux::key::name(&mapped, mods))
            .flatten();
        match named {
            Some(name) => {
                self.send_input(&std::mem::take(bytes));
                self.send_key(&name, &encoded);
            }
            None => bytes.extend(encoded),
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
        self.apply_scrollback();
        let overflow = self.settings.screen_overflow;
        self.session
            .set_screen_overflow((overflow.columns, overflow.rows));
        self.session
            .set_tmux_pause(self.settings.tmux.pause_after());
        self.apply_picture_limits(context);
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
        if self.session.take_params_of_run()
            && let Some(id) = self.active_port_id()
        {
            self.port_memory_mut(&id).line = self.session.params;
            self.save_memory(&SourceKey::Port(id));
        }
        if self.session.off() != self.applied_off {
            self.applied_off = self.session.off();
            self.apply_osc_settings();
        }
        self.act_on_replies(context);
        self.show_asked_dialog();
        self.follow_the_terminal_shown();
        self.refill_tmux_menu();
        self.write_down_commands();
        self.say_what_runs_came_to();
        self.handle_keyboard(context);
        self.handle_terminal_events(context);
        self.handle_dropped_files(context);
        self.handle_tasks(context);
        self.handle_jobs();

        self.schedule_repaint(context);
    }

    /// Hands all that holds a port, a program or a thread to the thread of
    /// the exit, so the window goes off the screen at once.
    fn on_exit(&mut self) {
        crate::exit::let_go((self.session.leave(), std::mem::take(&mut self.jobs)));
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

/// How large a file the window has just written came to, or nought when the
/// system will not say.
fn file_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|data| data.len()).unwrap_or(0)
}

/// Hands one line to the desktop, and says in the log what came of it.
///
/// Every notification of this program goes through here: the ones a program of
/// a session asks for and the ones this window has to say for itself. A desktop
/// that takes none of them is a line in the log and nothing else — there is
/// nowhere else to put it, and a window that stopped to say so would be worse
/// than a notification nobody saw.
fn notify_desktop(heading: &str, body: &str) {
    if let Err(error) = notify_rust::Notification::new()
        .summary(heading)
        .body(body)
        .show()
    {
        log::warn!("notification: {error}");
    }
}

/// A picture as the bytes of a PNG file.
///
/// It is this format and no other because it is the one a desktop opens, keeps
/// every pixel of what was drawn, and carries the transparency a sixel picture
/// has wherever the program painted nothing.
fn encode_png(pixels: &zyt_term::Pixels) -> std::result::Result<Vec<u8>, String> {
    use image::ImageEncoder;

    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &pixels.rgba,
            pixels.width as u32,
            pixels.height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| error.to_string())?;
    Ok(bytes)
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

/// The bytes of a command of several lines, with the carets read or not.
///
/// A device closes a line with a carriage return and not with a line feed, so
/// every break becomes one. What the window holds is left otherwise untouched:
/// the spaces of an indented block are the block.
///
/// The breaks are turned into carriage returns before the carets are read, so a
/// `^J` written by hand is the line feed it stands for and not a break of the
/// field read a second time.
///
/// What closes the block is the two switches of the plate:
///
/// | the carets | the last line |
/// | --- | --- |
/// | characters | closed by a return added here, unless the field ends in a break |
/// | control codes | closed by what is written there and by nothing else |
///
/// With them read, every control code of the block is one somebody wrote: a
/// `^M` at the end is the key that runs it, a break of the field is the same
/// key, and a block carrying neither is a block that was not to be closed — a
/// bootloader fed a key and a shell handed the end of its input end where the
/// text does.
fn typed_block(text: &str, caret: bool) -> Vec<u8> {
    let lines = text.replace('\n', "\r");
    if caret {
        return crate::caret::expand(&lines).into_bytes();
    }
    let mut bytes = lines.into_bytes();
    if !bytes.ends_with(b"\r") {
        bytes.push(b'\r');
    }
    bytes
}

/// The bytes a command of the history is typed back as.
///
/// With the carets of it read — `Entry::caret` — it is the text through
/// `crate::caret` and nothing else: the return that runs it is the key the
/// command was chosen by (`run_from_history`) and not a part of the command, so
/// nothing is added here and nothing is taken away.
fn typed_again(command: &str, caret: bool) -> Vec<u8> {
    match caret {
        true => crate::caret::expand(command).into_bytes(),
        false => command.as_bytes().to_vec(),
    }
}

/// Whether a command typed back from the history is closed by this side.
///
/// Only one chosen to be run is, and only while its carets stand as the
/// characters they are. With them read, every control code of the command is
/// one somebody wrote: the `^M` that ends it is written at the end, and a
/// command carrying none is a command that was not to be closed — a bootloader
/// fed a key and a shell handed the end of its input both end where the text
/// does.
fn closes_a_command(action: HistoryAction, caret: bool) -> bool {
    action == HistoryAction::Run && !caret
}

/// Whether a key is one of those that walk the grid.
///
/// These are the keys a selection of the keyboard is made with, so one of them
/// pressed without the modifiers is what says the selection is over. Every
/// other key leaves it standing: a line typed with a selection on the screen is
/// a line typed next to it.
/// Whether a clipboard event of the toolkit was made by a shortcut.
///
/// `egui` answers `ctrl+C`, `ctrl+X` and `ctrl+V` with the same events as the
/// `Copy`, `Cut` and `Paste` keys of a keyboard that carries them, and swallows
/// the key press either way, so the event alone does not say which was pressed.
/// What is held at that moment does: a shortcut holds the modifier and a key of
/// its own holds nothing.
fn clipboard_shortcut_held(modifiers: &egui::Modifiers) -> bool {
    modifiers.command || modifiers.ctrl
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

    /// The contents of a file are offered right after the clipboard, in the
    /// menu of the terminal and in the menu of the session alike.
    #[test]
    fn the_contents_of_a_file_follow_paste() {
        let (_, app) = alone();
        for items in [
            crate::ui::menu::terminal_items(&app),
            crate::ui::menu::session_items(&app),
        ] {
            let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
            let paste = ids
                .iter()
                .position(|id| *id == crate::commands::AppCommand::Paste.id())
                .expect("paste is offered");
            assert_eq!(ids.get(paste + 1), Some(&"terminal.insert_file"), "{ids:?}");
        }
    }

    /// An application with a store of its own, drawn without a window.
    ///
    /// Each call takes a directory nobody else has: these tests run beside
    /// each other, and a store two of them wrote to is a store neither of them
    /// describes.
    fn alone() -> (egui::Context, App) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let count = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("zyterm-app-{}-{count}", std::process::id()));
        let store = ConfigStore::with_paths(
            directory.join("config"),
            directory.join("data"),
            directory.join("lock"),
        );
        let context = egui::Context::default();
        let app = App::new(&context, store, crate::config::Settings::default(), None)
            .expect("the application starts");
        (context, app)
    }

    /// The line a picture leaves is a picture the menu acts on, it is the
    /// picture and nothing else, and a picture the budget has dropped is a line
    /// the menu leaves alone.
    #[test]
    fn the_menu_of_a_picture_is_the_picture_and_nothing_else() {
        let (_context, mut app) = alone();
        app.session
            .test_terminal()
            .feed(b"\x1bPq\"1;1;64;36#0;2;100;0;0#0~~~~$-\x1b\\");

        let content = app.session.test_terminal().content();
        let id = (0..content.columns)
            .find_map(|column| {
                content
                    .cell(column, 0)
                    .and_then(|cell| cell.image.map(|part| part.id))
            })
            .expect("the line of the picture names it");

        app.image_menu = app
            .session
            .test_terminal()
            .picture(id)
            .map(|picture| crate::app::PictureMenu { id, picture });
        let held = app
            .image_menu
            .as_ref()
            .expect("the picture is the one the cells name");
        assert_eq!(
            (held.picture.pixels.width, held.picture.pixels.height),
            (64, 36),
            "its pixels were read as it arrived"
        );

        let items = crate::ui::menu::image_items(&app);
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "image.title",
                "",
                "image.save",
                "image.save_open_with",
                "image.copy",
                "image.fold"
            ],
            "the menu is the picture and nothing of the terminal"
        );

        app.session.test_terminal().set_picture_budget(0);
        assert!(
            app.session.test_terminal().picture(id).is_none(),
            "a picture the budget has dropped is offered no menu"
        );
    }

    /// A picture larger than the window is shrunk to it and is still a
    /// picture: the cells name it and the menu has it to save.
    #[test]
    fn a_picture_larger_than_the_window_is_still_a_picture() {
        let (context, mut app) = alone();
        frame(&context, &mut app, Vec::new());
        app.session.settle();
        app.apply_picture_limits(&context);
        let (width, height) = app.terminal_pixels(&context);

        let payload = format!(
            "\x1bPq\"1;1;{};{}#0;2;100;0;0#0{}\x1b\\",
            width + 60,
            height + 60,
            format!("!{}~-", width + 60).repeat((height + 60).div_ceil(6)),
        );
        app.session.test_terminal().feed(payload.as_bytes());

        let content = app.session.test_terminal().content();
        let id = content
            .cell(0, 0)
            .and_then(|cell| cell.image.map(|part| part.id))
            .expect("the cells of the picture name it");
        let picture = app
            .session
            .test_terminal()
            .picture(id)
            .expect("the picture is kept");
        let shrunk = (picture.pixels.width, picture.pixels.height);
        assert!(
            shrunk.0 <= width && shrunk.1 <= height,
            "shrunk into the window it arrived in, {shrunk:?} in {width}x{height}"
        );
        assert!(
            shrunk.0 == width || shrunk.1 == height,
            "and as large as it fits, {shrunk:?} in {width}x{height}"
        );
    }

    /// One frame in the order a frame runs: the keys first, the drawing after.
    fn frame(context: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            app.handle_keyboard(context);
            crate::ui::draw(app, ui);
        });
        output.textures_delta.clear();
    }

    /// One key press as the toolkit reports it.
    fn pressed(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// `Ctrl` and `Shift` held, the way the desktop reports them on this side.
    fn ctrl_shift() -> egui::Modifiers {
        egui::Modifiers {
            shift: true,
            ctrl: true,
            command: true,
            ..Default::default()
        }
    }

    /// A key pressed while a selection stands lets it go and is typed into the
    /// device: somebody who picks something out and then types is done looking
    /// at it, and a press that only ended the mode would have to be made twice.
    #[test]
    fn a_key_pressed_while_a_selection_stands_lets_it_go_and_is_typed() {
        let (context, mut app) = alone();
        frame(&context, &mut app, Vec::new());

        app.session.test_terminal().feed(b"alpha beta\r\n");
        app.session.test_terminal().select_by_key(Step::Left);
        assert!(
            app.session.test_terminal().selecting(),
            "a selection stands"
        );

        let before = app.session.counters.bytes_out;
        frame(
            &context,
            &mut app,
            vec![
                pressed(egui::Key::X, egui::Modifiers::NONE),
                egui::Event::Text("x".to_string()),
            ],
        );

        assert!(!app.session.test_terminal().selecting(), "the mode is left");
        assert!(
            app.session.counters.bytes_out > before,
            "and the key went to the device"
        );
    }

    /// `Esc` is the one key that only ends the mode: it is what every other
    /// window of this program is left with, and a program on the line is owed
    /// that byte for its own reasons.
    #[test]
    fn escape_ends_the_mode_and_reaches_nothing() {
        let (context, mut app) = alone();
        frame(&context, &mut app, Vec::new());

        app.session.test_terminal().feed(b"alpha beta\r\n");
        app.session.test_terminal().select_by_key(Step::Left);

        let before = app.session.counters.bytes_out;
        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Escape, egui::Modifiers::NONE)],
        );

        assert!(!app.session.test_terminal().selecting());
        assert_eq!(app.session.counters.bytes_out, before, "nothing was sent");
    }

    /// A modifier pressed on its own sends nothing, so the selection stands:
    /// it is the first half of a combination, and `Ctrl` or `Alt` is what a
    /// block is picked out with.
    #[test]
    fn a_modifier_alone_leaves_the_selection_standing() {
        let (context, mut app) = alone();
        frame(&context, &mut app, Vec::new());

        app.session.test_terminal().feed(b"alpha beta\r\n");
        app.session.test_terminal().select_by_key(Step::Left);

        let before = app.session.counters.bytes_out;
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(
            &context,
            &mut app,
            vec![
                pressed(egui::Key::ControlLeft, ctrl),
                pressed(egui::Key::AltLeft, egui::Modifiers::ALT),
            ],
        );

        assert!(
            app.session.test_terminal().selecting(),
            "the selection stands"
        );
        assert_eq!(app.session.counters.bytes_out, before, "nothing was sent");
    }

    /// The middle button pastes the selection of this terminal when the
    /// platform has none, and the paste is input like any other: the text goes
    /// to the device and the selection it came from goes away.
    #[test]
    fn a_paste_of_the_selection_sends_it_and_lets_it_go() {
        let (context, mut app) = alone();
        frame(&context, &mut app, Vec::new());

        app.session.test_terminal().feed(b"alpha beta\r\n");
        app.session
            .test_terminal()
            .selection_start(zyt_term::SelectionKind::Lines, 0, 0)
            .expect("a selection");

        let before = app.session.counters.bytes_out;
        app.paste_selection_or_clipboard();
        app.session.settle();
        app.act_on_replies(&context);

        assert!(
            !app.session.test_terminal().selecting(),
            "the selection went"
        );
        assert!(
            app.session.counters.bytes_out > before,
            "and the paste was sent"
        );
    }

    /// A press in the terminal with nothing held lets the selection go. The
    /// widget does it, the way it does for a selection it made itself.
    #[test]
    fn a_press_in_the_terminal_lets_the_selection_go() {
        let (context, mut app) = alone();
        frame(&context, &mut app, Vec::new());

        app.session.test_terminal().feed(b"alpha beta\r\n");
        app.session.test_terminal().select_by_key(Step::Left);

        let at = egui::pos2(450.0, 300.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&context, &mut app, vec![egui::Event::PointerMoved(at)]);
        frame(&context, &mut app, vec![button(true)]);
        frame(&context, &mut app, vec![button(false)]);

        assert!(!app.session.test_terminal().selecting());
    }

    /// Empty input is no input: it sends nothing and ends nothing.
    #[test]
    fn empty_input_leaves_the_selection_standing() {
        let (context, mut app) = alone();
        frame(&context, &mut app, Vec::new());

        app.session.test_terminal().feed(b"alpha beta\r\n");
        app.session.test_terminal().select_by_key(Step::Left);

        app.send_input(&[]);

        assert!(app.session.test_terminal().selecting());
    }

    #[test]
    fn the_plate_of_a_command_of_several_lines_opens_on_ctrl_shift_enter() {
        let (context, mut app) = alone();

        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Enter, ctrl_shift())],
        );

        assert_eq!(app.ui.block.as_deref(), Some(""));
    }

    #[test]
    fn the_press_that_opened_it_is_not_read_again_by_the_field() {
        let (context, mut app) = alone();

        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Enter, ctrl_shift())],
        );
        frame(&context, &mut app, Vec::new());

        assert_eq!(app.ui.block.as_deref(), Some(""));
    }

    #[test]
    fn escape_closes_the_plate_once_the_field_holds_the_keyboard() {
        let (context, mut app) = alone();

        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Enter, ctrl_shift())],
        );
        frame(&context, &mut app, Vec::new());
        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Escape, egui::Modifiers::NONE)],
        );

        assert_eq!(app.ui.block, None);
    }

    #[test]
    fn ctrl_enter_sends_what_the_plate_holds_and_takes_it_down() {
        let (context, mut app) = alone();

        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Enter, ctrl_shift())],
        );
        app.ui.block = Some("echo one\necho two".to_string());
        frame(&context, &mut app, Vec::new());
        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Enter, egui::Modifiers::COMMAND)],
        );

        assert_eq!(app.ui.block, None);
    }

    #[test]
    fn the_combination_that_opens_the_plate_does_not_send_what_is_in_it() {
        let (context, mut app) = alone();

        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Enter, ctrl_shift())],
        );
        app.ui.block = Some("echo one".to_string());
        frame(&context, &mut app, Vec::new());
        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Enter, ctrl_shift())],
        );

        assert_eq!(app.ui.block.as_deref(), Some("echo one"));
    }

    #[test]
    fn the_plate_stands_against_the_edge_it_was_sent_to_and_never_over_the_bar() {
        let (context, mut app) = alone();
        let plate = egui::Id::new("block").with("plate");

        frame(
            &context,
            &mut app,
            vec![pressed(egui::Key::Enter, ctrl_shift())],
        );
        frame(&context, &mut app, Vec::new());
        let foot = context
            .memory(|memory| memory.area_rect(plate))
            .expect("the plate was drawn");
        let content = context.content_rect();

        app.ui.block_at_top = true;
        frame(&context, &mut app, Vec::new());
        let head = context
            .memory(|memory| memory.area_rect(plate))
            .expect("the plate was drawn");

        app.ui.block_at_top = false;
        app.settings.show_status_bar = false;
        frame(&context, &mut app, Vec::new());
        let bare = context
            .memory(|memory| memory.area_rect(plate))
            .expect("the plate was drawn");

        assert!(
            foot.top() > content.center().y,
            "{foot:?} is not at the foot"
        );
        assert!(
            head.bottom() < content.center().y,
            "{head:?} is not at the head"
        );
        assert_eq!(head.width(), foot.width());
        assert!(
            foot.bottom() < bare.bottom(),
            "{foot:?} stands as low as a window with no status bar ({bare:?})"
        );
    }

    #[test]
    fn every_line_of_a_block_is_closed_with_a_carriage_return() {
        assert_eq!(typed_block("ls", false), b"ls\r");
        assert_eq!(
            typed_block("for i in 1 2\ndo\necho $i\ndone", false),
            b"for i in 1 2\rdo\recho $i\rdone\r"
        );
        assert_eq!(typed_block("ls\n", false), b"ls\r");
        assert_eq!(
            typed_block("  indented\n  lines", false),
            b"  indented\r  lines\r"
        );
    }

    #[test]
    fn a_caret_is_a_caret_while_the_switch_is_off() {
        assert_eq!(typed_block("echo 2^3", false), b"echo 2^3\r");
    }

    #[test]
    fn the_switch_turns_a_caret_sequence_into_its_byte() {
        assert_eq!(typed_block("sleep 9\n^C", true), b"sleep 9\r\x03");
        assert_eq!(typed_block("cat\nline\n^D", true), b"cat\rline\r\x04");
        assert_eq!(typed_block("^[", true), b"\x1b");
    }

    /// With the carets read, the block ends where the text does: the return
    /// that runs it is written as `^M` or as a break of the field, and nothing
    /// is added to a block that carries neither.
    #[test]
    fn a_block_of_read_carets_is_closed_by_what_is_written_in_it() {
        assert_eq!(typed_block("^Cls", true), b"\x03ls");
        assert_eq!(typed_block("ls^M", true), b"ls\r");
        assert_eq!(typed_block("ls\n", true), b"ls\r");
        assert_eq!(typed_block("ls", true), b"ls");
    }

    /// A command typed back is the text of the entry, read the way the entry
    /// says it was read when it ran — and closed by nothing: the key it was
    /// chosen by is what runs it.
    #[test]
    fn a_command_of_the_history_is_typed_back_as_the_entry_says_it_ran() {
        assert_eq!(typed_again("echo 2^3", false), b"echo 2^3");
        assert_eq!(typed_again("sleep 9\r^C", true), b"sleep 9\r\x03");
        assert_eq!(
            typed_again("^C", false),
            b"^C",
            "the carets stand as written"
        );
    }

    #[test]
    fn a_held_modifier_says_a_clipboard_event_came_from_a_shortcut() {
        assert!(clipboard_shortcut_held(&egui::Modifiers::COMMAND));
        assert!(clipboard_shortcut_held(&egui::Modifiers::CTRL));
        assert!(clipboard_shortcut_held(&egui::Modifiers {
            shift: true,
            ctrl: true,
            command: true,
            ..Default::default()
        }));
    }

    #[test]
    fn a_clipboard_key_of_its_own_holds_nothing() {
        assert!(!clipboard_shortcut_held(&egui::Modifiers::NONE));
        assert!(!clipboard_shortcut_held(&egui::Modifiers::SHIFT));
        assert!(!clipboard_shortcut_held(&egui::Modifiers::ALT));
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

    /// A value typed into the window that asks before a console is opened is
    /// what that connection stands on, so everything asking the source by name
    /// is answered from it: a script wanting `remote_host` is offered on a
    /// connection that was told one, and not only on a source whose settings
    /// carry it.
    #[test]
    fn what_was_answered_on_the_way_in_answers_a_script_as_well() {
        let (_context, mut app) = alone();
        let mut console = crate::consoles::default_console();
        console.name = "SSH {remote_host}".to_string();
        console.program = "ssh {remote_user}@{remote_host}".to_string();
        let key = console.key();
        app.consoles.push(console);

        let entry = app
            .scripts
            .find("shell-driven-scp")
            .expect("the shipped script is there")
            .clone();

        assert_eq!(
            app.missing_variables_of(&entry, Some(&key)),
            ["remote_user", "remote_host"],
            "a source nothing answered wants both of them"
        );

        app.answered.insert(
            key.clone(),
            [
                ("remote_user".to_string(), "root".to_string()),
                ("remote_host".to_string(), "board-7".to_string()),
            ]
            .into_iter()
            .collect(),
        );

        assert!(
            app.missing_variables_of(&entry, Some(&key)).is_empty(),
            "what was answered on the way in is an answer"
        );
        assert_eq!(
            app.variables_of(Some(&key))
                .get("remote_host")
                .map(String::as_str),
            Some("board-7")
        );
        assert!(
            app.offered_scripts_of(Some(&key))
                .iter()
                .any(|offered| offered.id == entry.id),
            "a script whose values the connection answers is offered"
        );
    }
}
