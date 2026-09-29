//! Lists the settings open as a menu of plates.
//!
//! A list to choose from is one widget in this program, the menu of plates, and
//! a list folded into a row of the settings is the same list: the keys walk it,
//! typing searches it, and the entry in use is marked. What a row shows is the
//! value in use and nothing else, so the row is a button and the list is the
//! menu it opens.
//!
//! The menu answers a frame later than the click, which is why nothing here
//! writes into the copy of the settings the panel is drawing: [`apply`] is
//! given the application itself and changes what was chosen, saves it, and does
//! what the change asks for.

use crate::app::App;
use plate_menu::MenuItem;
use rust_i18n::t;
use zyt_serial::LineHold;

/// Prefix of an entry that names a value of the settings.
pub const CHOICE: &str = "choice:";

/// One list of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Palette of one color mode.
    TerminalTheme {
        /// Whether it is the palette of the dark mode.
        dark: bool,
    },
    /// Font family of the interface.
    Font,
    /// One more family for the chain the terminal is drawn with.
    FontAdd,
    /// What a program may do with the clipboard, on one kind of session.
    Clipboard {
        /// Whether it is the setting of a session that is not trusted.
        untrusted: bool,
    },
    /// Speed and framing of the line, as one menu of four lists.
    LineParams,
    /// How the line is held back when the other side cannot keep up.
    FlowControl,
    /// What this side does with one of the two lines it drives.
    LineHold {
        /// Whether it is the list of Data Terminal Ready.
        dtr: bool,
    },
    /// Which port or console the connection settings are showing.
    SettingsSource,
    /// Key sent to the device once a transfer is over.
    Finish {
        /// Profile the key belongs to.
        profile: usize,
        /// Whether it is the key of the receiving direction.
        receive: bool,
    },
}

impl Choice {
    /// What every entry of this list starts with, which is what tells the lists
    /// apart when a choice comes back.
    fn prefix(self) -> String {
        match self {
            Self::TerminalTheme { dark } => {
                format!("{CHOICE}theme:{}:", if dark { "dark" } else { "light" })
            }
            Self::Font => format!("{CHOICE}font:interface:"),
            Self::FontAdd => format!("{CHOICE}font:add:"),
            Self::Clipboard { untrusted } => format!(
                "{CHOICE}clipboard:{}:",
                if untrusted { "untrusted" } else { "trusted" }
            ),
            Self::LineParams => format!("{CHOICE}line:"),
            Self::SettingsSource => format!("{CHOICE}source::"),
            Self::FlowControl => format!("{CHOICE}flow::"),
            Self::LineHold { dtr } => {
                format!("{CHOICE}hold:{}:", if dtr { "dtr" } else { "rts" })
            }
            Self::Finish { profile, receive } => format!(
                "{CHOICE}finish:{}{profile}:",
                if receive { "receive" } else { "send" }
            ),
        }
    }

    /// The entry the value in use stands on, which is where the menu opens.
    fn current(self, app: &App) -> String {
        let prefix = self.prefix();
        match self {
            Self::TerminalTheme { dark } => {
                format!("{prefix}{}", app.settings.themes.name(dark))
            }
            Self::Font => {
                let chosen = app.settings.fonts.interface.as_deref();
                format!("{prefix}{}", chosen.unwrap_or_default())
            }
            Self::FontAdd => prefix,
            Self::Clipboard { untrusted } => {
                format!("{prefix}{}", clipboard_slug(clipboard_of(app, untrusted)))
            }
            Self::LineParams => format!("{prefix}baud:{}", app.session.params.baud_rate),
            Self::SettingsSource => match &app.settings_source {
                Some(key) => format!("{prefix}{key}"),
                None => prefix,
            },
            Self::FlowControl => {
                format!("{prefix}{}", flow_slug(app.session.params.flow_control))
            }
            Self::LineHold { dtr } => format!("{prefix}{}", hold_slug(hold_of(app, dtr))),
            Self::Finish { profile, receive } => {
                let finish = finish_of(app, profile, receive).unwrap_or_default();
                let custom =
                    !finish.is_empty() && !zyt_xfer::FINISH_PRESETS.contains(&finish.as_str());
                match custom {
                    true => format!("{prefix}{CUSTOM}"),
                    false => format!("{prefix}{finish}"),
                }
            }
        }
    }

    /// The list itself, with the value in use marked.
    fn items(self, app: &mut App) -> Vec<MenuItem> {
        match self {
            Self::TerminalTheme { dark } => theme_items(app, dark),
            Self::Font => font_items(app),
            Self::FontAdd => font_add_items(app),
            Self::Clipboard { untrusted } => clipboard_items(app, untrusted),
            Self::LineParams => line_items(app),
            Self::SettingsSource => source_items(app),
            Self::FlowControl => flow_items(app),
            Self::LineHold { dtr } => hold_items(app, dtr),
            Self::Finish { profile, receive } => finish_items(app, profile, receive),
        }
    }
}

/// The row that opens one of these lists: a button showing the value in use.
///
/// It is a button and nothing more — no mark of a list beside it and no width
/// of its own — because what it opens is the menu of this program, the same one
/// every other button opens. The menu opens on the value in use, so the
/// neighbouring values are one key away.
pub fn row(ui: &mut egui::Ui, app: &mut App, choice: Choice, shown: &str) {
    if ui.button(shown).clicked() {
        open(app, choice);
    }
}

/// Opens one of these lists on the value in use.
///
/// A row is the button most of them are opened by, and the modem lines of the
/// status bar are not: those carry the colour of the line and the mark of a
/// line driven from here, which no button showing a value does. So the opening
/// is a call of its own, and a caller may draw whatever it presses.
pub fn open(app: &mut App, choice: Choice) {
    let current = choice.current(app);
    let items = choice.items(app);
    let at = opens_at(&items, &current);
    if let Err(error) = app.menu.open_at(items, &at) {
        log::debug!("settings menu: {error}");
    }
}

/// The entry the menu opens on.
///
/// A list says twice which of its entries is the one in use: the entry carries
/// the mark, and the list is asked for the value beside it. The two are built
/// apart, and a list whose value is written one way and whose entries are named
/// another opens at the top — which is the one place a menu of values must not
/// open, because the value in use is what somebody came to see.
///
/// So the mark is what is looked for, and the name only when nothing is marked.
/// The mark is what the reader sees, so a menu that opens on it opens where the
/// list says it should whatever the two of them disagree about.
///
/// An entry that cannot be chosen is passed over even when it carries the mark:
/// a font the system no longer has stands in its list saying it is the one in
/// use and cannot be picked again, and a menu opening on a row nothing can be
/// done with opens nowhere.
fn opens_at(items: &[MenuItem], current: &str) -> String {
    items
        .iter()
        .find(|item| item.detail == crate::ui::icons::CURRENT && item.is_reachable())
        .map(|item| item.id.clone())
        .unwrap_or_else(|| current.to_string())
}

/// Acts on an entry of one of these lists, a frame after it was chosen.
///
/// Everything here changes the settings of the application rather than the copy
/// the panel is drawing, saves them, and asks for what the value needs: a
/// palette is resolved again, a font is installed again, a sequence setting
/// reaches the terminal.
pub fn apply(app: &mut App, id: &str) {
    let Some(rest) = id.strip_prefix(CHOICE) else {
        return;
    };
    let mut parts = rest.splitn(3, ':');
    let (Some(kind), Some(slot), Some(value)) = (parts.next(), parts.next(), parts.next()) else {
        return;
    };

    match kind {
        "theme" => apply_theme(app, slot == "dark", value),
        "font" => apply_font(app, slot, value),
        "clipboard" => apply_clipboard(app, slot == "untrusted", value),
        "line" => apply_line(app, slot, value),
        "source" => app.settings_source = value.parse().ok(),
        "flow" => apply_flow(app, value),
        "hold" => apply_hold(app, slot == "dtr", value),
        "finish" => apply_finish(app, slot, value),
        _ => log::debug!("settings menu: {id} names no list"),
    }
}

/// Every palette there is, for one color mode.
fn theme_items(app: &mut App, dark: bool) -> Vec<MenuItem> {
    let prefix = Choice::TerminalTheme { dark }.prefix();
    let current = app.settings.themes.name(dark).to_string();
    app.themes
        .names()
        .map(str::to_string)
        .collect::<Vec<String>>()
        .into_iter()
        .map(|name| {
            let mark = mark(name == current);
            MenuItem::new(format!("{prefix}{name}"), &name).detail(mark)
        })
        .collect()
}

/// Every family the system has, with the built-in font first.
///
/// All of them are offered and not the monospaced ones alone: the interface is
/// prose — the names of the settings, their hints, the words on the buttons —
/// and prose is read in whatever font somebody reads best, which is a question
/// about the eyes of the reader and not about the shape of a grid. A family
/// the system no longer has is still shown while it is the chosen one, and says
/// so, but it cannot be chosen again.
fn font_items(app: &mut App) -> Vec<MenuItem> {
    let prefix = Choice::Font.prefix();
    let current = app.settings.fonts.interface.clone();
    let installed: Vec<crate::fonts::Family> = app.font_families().to_vec();

    let mut items = vec![
        MenuItem::new(prefix.clone(), t!("settings.font_builtin")).detail(mark(current.is_none())),
    ];

    if let Some(name) = current.as_ref()
        && !installed.iter().any(|family| &family.name == name)
    {
        items.push(gone(&prefix, name));
    }

    for family in &installed {
        let name = family.name.as_str();
        items.push(
            MenuItem::new(format!("{prefix}{name}"), name)
                .detail(mark(current.as_deref() == Some(name))),
        );
    }

    items
}

/// Every monospaced family the chain of the terminal has not got yet.
///
/// Only the monospaced ones are offered: the terminal is a grid of cells, and a
/// family whose glyphs are of every width is a grid with the letters standing
/// beside their cells — wherever in the chain it is asked, because a link that
/// is not the first is asked for whole characters just the same.
///
/// A family already in the chain is not offered: it is in it once, and a second
/// link naming it would be a link nothing is ever asked of.
fn font_add_items(app: &mut App) -> Vec<MenuItem> {
    let prefix = Choice::FontAdd.prefix();
    let chain = app.settings.fonts.terminal.clone();

    app.font_families()
        .iter()
        .filter(|family| family.monospaced)
        .filter(|family| !chain.iter().any(|name| name == &family.name))
        .map(|family| MenuItem::new(format!("{prefix}{}", family.name), family.name.as_str()))
        .collect()
}

/// The entry of a family the system no longer has, which names it and cannot be
/// chosen.
///
/// A font somebody uninstalled is still the setting it was, so the list says it
/// is the one in use rather than quietly showing something else in its place.
fn gone(prefix: &str, name: &str) -> MenuItem {
    MenuItem::new(
        format!("{prefix}{name}"),
        format!("{name} \u{2014} {}", t!("settings.font_gone")),
    )
    .detail(crate::ui::icons::CURRENT)
    .enabled(false)
}

/// What a program may do with the clipboard, on one kind of session.
fn clipboard_items(app: &mut App, untrusted: bool) -> Vec<MenuItem> {
    let prefix = Choice::Clipboard { untrusted }.prefix();
    let current = clipboard_of(app, untrusted);

    crate::config::CLIPBOARD_SETTINGS
        .iter()
        .copied()
        .map(|setting| {
            MenuItem::new(
                format!("{prefix}{}", clipboard_slug(setting)),
                t!(setting.label_key()),
            )
            .detail(mark(setting == current))
            .full(t!(setting.hint_key()))
        })
        .collect()
}

/// Nothing, every key a profile offers, and a sequence of one's own.
fn finish_items(app: &mut App, profile: usize, receive: bool) -> Vec<MenuItem> {
    let prefix = Choice::Finish { profile, receive }.prefix();
    let current = finish_of(app, profile, receive).unwrap_or_default();
    let custom = !current.is_empty() && !zyt_xfer::FINISH_PRESETS.contains(&current.as_str());

    let mut items = vec![
        MenuItem::new(prefix.clone(), t!("settings.finish_none")).detail(mark(current.is_empty())),
    ];
    for preset in zyt_xfer::FINISH_PRESETS {
        items.push(
            MenuItem::new(format!("{prefix}{preset}"), *preset).detail(mark(current == *preset)),
        );
    }
    items.push(
        MenuItem::new(format!("{prefix}{CUSTOM}"), t!("settings.finish_custom"))
            .hint(t!("settings.finish_hint"))
            .detail(mark(custom)),
    );
    items
}

/// The consoles and the port this window is on.
///
/// The one the page is showing is not an entry of its own. A menu of this
/// program opens on the value in use — that is what `open_at` is for — so the
/// list stands with that one under the cursor and its neighbours a key away,
/// and an entry saying what the cursor is already resting on would be a line
/// spent saying it twice.
fn source_items(app: &App) -> Vec<MenuItem> {
    let prefix = Choice::SettingsSource.prefix();
    let shown = app.settings_source_key();

    app.known_sources()
        .into_iter()
        .map(|(key, label)| {
            let mark = mark(shown.as_ref() == Some(&key));
            MenuItem::new(format!("{prefix}{key}"), label)
                .detail(mark)
                .full(key.to_string())
        })
        .collect()
}

/// Speed and framing.
///
/// The speed is the list itself: it is the value that is changed, and a line
/// that is answered at the wrong speed says nothing at all. The framing is
/// three entries below it, each carrying its own values and showing the one in
/// use, because it is set once for a device and then left alone. Each of the
/// three opens on the value it stands on, the way the menu itself opens on the
/// speed in use.
fn line_items(app: &App) -> Vec<MenuItem> {
    let prefix = Choice::LineParams.prefix();
    let params = app.session.params;

    let rates = app.baud_rates_of(app.active_port_id().as_ref());
    let mut items: Vec<MenuItem> = rates
        .iter()
        .map(|rate| {
            MenuItem::new(format!("{prefix}baud:{rate}"), rate.to_string())
                .detail(mark(params.baud_rate == *rate))
        })
        .collect();

    let data: Vec<MenuItem> = [zyt_serial::DataBits::Seven, zyt_serial::DataBits::Eight]
        .into_iter()
        .map(|bits| {
            MenuItem::new(format!("{prefix}data:{bits:?}"), format!("{bits:?}"))
                .detail(mark(params.data_bits == bits))
        })
        .collect();

    let parity: Vec<MenuItem> = [
        zyt_serial::Parity::None,
        zyt_serial::Parity::Even,
        zyt_serial::Parity::Odd,
    ]
    .into_iter()
    .map(|parity| {
        MenuItem::new(format!("{prefix}parity:{parity:?}"), format!("{parity:?}"))
            .detail(mark(params.parity == parity))
    })
    .collect();

    let stop: Vec<MenuItem> = [zyt_serial::StopBits::One, zyt_serial::StopBits::Two]
        .into_iter()
        .map(|stop| {
            MenuItem::new(format!("{prefix}stop:{stop:?}"), format!("{stop:?}"))
                .detail(mark(params.stop_bits == stop))
        })
        .collect();

    items.push(MenuItem::separator());
    items.push(
        MenuItem::new(format!("{prefix}data"), t!("settings.data_bits"))
            .detail(format!("{:?}", params.data_bits))
            .opens_at(format!("{prefix}data:{:?}", params.data_bits))
            .children(data),
    );
    items.push(
        MenuItem::new(format!("{prefix}parity"), t!("settings.parity"))
            .detail(format!("{:?}", params.parity))
            .opens_at(format!("{prefix}parity:{:?}", params.parity))
            .children(parity),
    );
    items.push(
        MenuItem::new(format!("{prefix}stop"), t!("settings.stop_bits"))
            .detail(format!("{:?}", params.stop_bits))
            .opens_at(format!("{prefix}stop:{:?}", params.stop_bits))
            .children(stop),
    );
    items
}

/// Every way of holding the line back.
fn flow_items(app: &App) -> Vec<MenuItem> {
    let prefix = Choice::FlowControl.prefix();
    let current = app.session.params.flow_control;

    FLOW_MODES
        .into_iter()
        .map(|mode| {
            MenuItem::new(format!("{prefix}{}", flow_slug(mode)), flow_label(mode))
                .detail(mark(current == mode))
        })
        .collect()
}

/// The three things that can be done with a line this side drives.
///
/// Automatic first, because it is what a port opens as and what a line under
/// hardware flow control has to stay on. The two forced modes follow, down
/// before up, which is the order the levels are named in everywhere else.
fn hold_items(app: &App, dtr: bool) -> Vec<MenuItem> {
    let prefix = Choice::LineHold { dtr }.prefix();
    let current = hold_of(app, dtr);

    [LineHold::Auto, LineHold::Down, LineHold::Up]
        .into_iter()
        .map(|hold| {
            MenuItem::new(format!("{prefix}{}", hold_slug(hold)), hold_label(hold))
                .detail(mark(current == hold))
                .full(t!(hold_hint_key(hold)))
        })
        .collect()
}

/// What this side does with one of the two lines it drives.
fn hold_of(app: &App, dtr: bool) -> LineHold {
    match dtr {
        true => app.session.dtr_hold,
        false => app.session.rts_hold,
    }
}

/// Name of a hold inside an entry.
fn hold_slug(hold: LineHold) -> &'static str {
    match hold {
        LineHold::Auto => "auto",
        LineHold::Down => "down",
        LineHold::Up => "up",
    }
}

/// The hold an entry names, which is nothing where it names none.
fn hold_of_slug(value: &str) -> Option<LineHold> {
    [LineHold::Auto, LineHold::Down, LineHold::Up]
        .into_iter()
        .find(|hold| hold_slug(*hold) == value)
}

/// What a hold is called.
pub fn hold_label(hold: LineHold) -> String {
    match hold {
        LineHold::Auto => t!("hold.auto").to_string(),
        LineHold::Down => t!("hold.down").to_string(),
        LineHold::Up => t!("hold.up").to_string(),
    }
}

/// The sentence that says what a hold does.
fn hold_hint_key(hold: LineHold) -> &'static str {
    match hold {
        LineHold::Auto => "hold.auto_hint",
        LineHold::Down => "hold.down_hint",
        LineHold::Up => "hold.up_hint",
    }
}

/// Name of a flow control setting inside an entry.
fn flow_slug(mode: zyt_serial::FlowControl) -> &'static str {
    match mode {
        zyt_serial::FlowControl::None => "none",
        zyt_serial::FlowControl::Hardware => "hardware",
        zyt_serial::FlowControl::Software => "software",
        zyt_serial::FlowControl::Both => "both",
    }
}

/// The way of holding the line back an entry names, which is nothing where it
/// names none.
fn flow_of_slug(value: &str) -> Option<zyt_serial::FlowControl> {
    FLOW_MODES
        .into_iter()
        .find(|mode| flow_slug(*mode) == value)
}

/// Every way of holding the line back, in the order the list offers them.
const FLOW_MODES: [zyt_serial::FlowControl; 4] = [
    zyt_serial::FlowControl::None,
    zyt_serial::FlowControl::Hardware,
    zyt_serial::FlowControl::Software,
    zyt_serial::FlowControl::Both,
];

/// What a flow control setting is called.
pub fn flow_label(mode: zyt_serial::FlowControl) -> String {
    match mode {
        zyt_serial::FlowControl::None => t!("flow.none").to_string(),
        zyt_serial::FlowControl::Hardware => t!("flow.hardware").to_string(),
        zyt_serial::FlowControl::Software => t!("flow.software").to_string(),
        zyt_serial::FlowControl::Both => t!("flow.both").to_string(),
    }
}

/// The value that stands for a sequence the user writes out.
const CUSTOM: &str = "custom";

/// The sequence a key of one's own starts from, which is what it was before
/// this menu as well.
const CUSTOM_START: &str = "\\x03";

/// Mark of the value in use.
fn mark(current: bool) -> &'static str {
    if current {
        crate::ui::icons::CURRENT
    } else {
        ""
    }
}

/// Name of a clipboard setting inside an entry.
fn clipboard_slug(setting: crate::config::ClipboardSetting) -> &'static str {
    match setting {
        crate::config::ClipboardSetting::Disabled => "disabled",
        crate::config::ClipboardSetting::CopyOnly => "copy_only",
        crate::config::ClipboardSetting::CopyLimitedPaste => "copy_limited_paste",
        crate::config::ClipboardSetting::CopyPaste => "copy_paste",
    }
}

/// The clipboard setting an entry names, which is nothing when it names none.
fn clipboard_of_slug(value: &str) -> Option<crate::config::ClipboardSetting> {
    crate::config::CLIPBOARD_SETTINGS
        .iter()
        .copied()
        .find(|setting| clipboard_slug(*setting) == value)
}

/// The clipboard setting of one kind of session.
fn clipboard_of(app: &App, untrusted: bool) -> crate::config::ClipboardSetting {
    match untrusted {
        true => app.settings.osc.untrusted.clipboard,
        false => app.settings.osc.trusted.clipboard,
    }
}

/// The key one direction of one profile sends when it is done.
fn finish_of(app: &App, profile: usize, receive: bool) -> Option<String> {
    let profile = app.profiles.get(profile)?;
    let commands = match receive {
        true => &profile.receive,
        false => &profile.send,
    };
    Some(commands.finish.clone())
}

fn apply_theme(app: &mut App, dark: bool, name: &str) {
    if app.settings.themes.name(dark) == name {
        return;
    }
    *app.settings.themes.name_mut(dark) = name.to_string();
    app.save_settings();
    app.apply_terminal_theme();
}

/// Sets the font of the interface, or adds one more family to the chain of the
/// terminal.
///
/// The chain is added to at its end, because that is what the list it comes
/// from offers: the families in front of it are the ones already collected, and
/// where a new one belongs among them is what the buttons of the row say, not
/// this list.
fn apply_font(app: &mut App, slot: &str, name: &str) {
    match slot {
        "interface" => {
            let chosen = (!name.is_empty()).then(|| name.to_string());
            if app.settings.fonts.interface == chosen {
                return;
            }
            app.settings.fonts.interface = chosen;
        }
        "add" => {
            let chain = &mut app.settings.fonts.terminal;
            if name.is_empty() || chain.iter().any(|link| link == name) {
                return;
            }
            chain.push(name.to_string());
        }
        _ => return,
    }
    app.save_settings();
    app.mark_fonts_dirty();
}

fn apply_clipboard(app: &mut App, untrusted: bool, value: &str) {
    let Some(setting) = clipboard_of_slug(value) else {
        return;
    };
    if clipboard_of(app, untrusted) == setting {
        return;
    }
    let side = match untrusted {
        true => &mut app.settings.osc.untrusted,
        false => &mut app.settings.osc.trusted,
    };
    side.clipboard = setting;
    app.save_settings();
    app.apply_osc_settings();
}

fn apply_line(app: &mut App, slot: &str, value: &str) {
    let mut params = app.session.params;
    match slot {
        "baud" => match value.parse::<u32>() {
            Ok(rate) => params.baud_rate = rate,
            Err(_) => return,
        },
        "data" => {
            params.data_bits = match value {
                "Seven" => zyt_serial::DataBits::Seven,
                "Eight" => zyt_serial::DataBits::Eight,
                _ => return,
            }
        }
        "parity" => {
            params.parity = match value {
                "None" => zyt_serial::Parity::None,
                "Even" => zyt_serial::Parity::Even,
                "Odd" => zyt_serial::Parity::Odd,
                _ => return,
            }
        }
        "stop" => {
            params.stop_bits = match value {
                "One" => zyt_serial::StopBits::One,
                "Two" => zyt_serial::StopBits::Two,
                _ => return,
            }
        }
        _ => return,
    }
    app.set_line_params(params);
}

fn apply_flow(app: &mut App, value: &str) {
    let Some(mode) = flow_of_slug(value) else {
        return;
    };
    let mut params = app.session.params;
    params.flow_control = mode;
    app.set_line_params(params);
}

fn apply_hold(app: &mut App, dtr: bool, value: &str) {
    let Some(hold) = hold_of_slug(value) else {
        return;
    };
    if hold_of(app, dtr) == hold {
        return;
    }
    app.set_line_hold(dtr, hold);
}

fn apply_finish(app: &mut App, slot: &str, value: &str) {
    let (receive, index) = match slot.strip_prefix("receive") {
        Some(index) => (true, index),
        None => (false, slot.strip_prefix("send").unwrap_or_default()),
    };
    let Ok(index) = index.parse::<usize>() else {
        return;
    };
    let Some(profile) = app.profiles.get_mut(index) else {
        return;
    };
    let commands = match receive {
        true => &mut profile.receive,
        false => &mut profile.send,
    };

    let custom = !commands.finish.is_empty()
        && !zyt_xfer::FINISH_PRESETS.contains(&commands.finish.as_str());
    let chosen = match value {
        CUSTOM if custom => return,
        CUSTOM => CUSTOM_START.to_string(),
        preset => preset.to_string(),
    };
    if commands.finish == chosen {
        return;
    }
    commands.finish = chosen;
    app.save_profiles();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every clipboard setting is named in an entry and read back from it: a
    /// setting the entries offer and the menu cannot resolve would be a plate
    /// that changes nothing when it is chosen.
    #[test]
    fn every_clipboard_setting_survives_its_entry() {
        for setting in crate::config::CLIPBOARD_SETTINGS.iter().copied() {
            let slug = clipboard_slug(setting);

            assert_eq!(clipboard_of_slug(slug), Some(setting), "{slug}");
        }

        assert_eq!(clipboard_of_slug("no such setting"), None);
    }

    /// Every way of holding the line back is named in an entry and read back
    /// from it: a mode the entries offer and `apply` cannot resolve would be a
    /// plate that changes nothing when it is chosen.
    #[test]
    fn every_flow_control_mode_survives_its_entry() {
        for mode in FLOW_MODES {
            let slug = flow_slug(mode);

            assert_eq!(flow_of_slug(slug), Some(mode), "{slug}");
        }

        assert_eq!(flow_of_slug("no such mode"), None);
    }

    /// Every hold is named in an entry and read back from it, the same way.
    #[test]
    fn every_hold_survives_its_entry() {
        for hold in [LineHold::Auto, LineHold::Down, LineHold::Up] {
            let slug = hold_slug(hold);

            assert_eq!(hold_of_slug(slug), Some(hold), "{slug}");
        }

        assert_eq!(hold_of_slug("no such hold"), None);
    }

    fn marked(id: &str) -> MenuItem {
        MenuItem::new(id, id).detail(crate::ui::icons::CURRENT)
    }

    #[test]
    fn a_menu_opens_on_the_entry_that_carries_the_mark() {
        let items = vec![
            MenuItem::new("first", "first"),
            marked("second"),
            MenuItem::new("third", "third"),
        ];

        assert_eq!(opens_at(&items, "names nothing"), "second");
    }

    #[test]
    fn a_marked_entry_nothing_can_be_done_with_is_passed_over() {
        let items = vec![marked("gone").enabled(false), MenuItem::new("here", "here")];

        assert_eq!(
            opens_at(&items, "here"),
            "here",
            "the name is what is left when the mark stands on a row nobody can pick"
        );
    }

    #[test]
    fn a_list_with_nothing_marked_opens_on_the_name_it_was_given() {
        let items = vec![
            MenuItem::new("first", "first"),
            MenuItem::new("last", "last"),
        ];

        assert_eq!(opens_at(&items, "last"), "last");
    }
}
