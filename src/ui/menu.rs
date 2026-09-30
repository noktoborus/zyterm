//! The menu of plates: what each of them offers and what a choice does.
//!
//! One widget serves the context menu of the terminal, the menu of a link, the
//! command palette, the command history and the kinds of search. What was
//! chosen comes back as the identifier of the entry, which is the identifier of
//! a command, a link action, a transfer profile, a way of reading a query, a
//! command of the history or the removal of one.

use crate::app::{App, shortcut_or_empty};
use crate::commands::AppCommand;
use plate_menu::MenuItem;
use rust_i18n::t;

/// Entry of the session menu that opens the settings about this connection.
const CONNECTION_SETTINGS: &str = "settings.connection";

/// Entry that writes the selection into a file.
const SAVE_SELECTION: &str = "selection.save";
/// Entry that writes it into a file and asks the desktop to open that file.
const SAVE_SELECTION_AS: &str = "selection.save_open_with";
/// Entry that puts the selection among the commands added by hand.
const REMEMBER_SELECTION: &str = "selection.remember";

/// Prefix of an entry that names a transfer profile.
const PROFILE: &str = "profile:";
/// Prefix of an entry that names a way of reading a search query.
const SEARCH_KIND: &str = "search.kind:";
/// Prefix of an entry of the menu about a source the window cannot reach.
const LOST: &str = "lost.";

/// Draws the menu and acts on the entry that was chosen.
///
/// A menu takes the keyboard from every widget while it stands, so the field of
/// the search is handed it back when the menu closes: the kinds of a query are
/// chosen from a menu, and the field they belong to is the one place here that
/// keeps the keyboard of the toolkit rather than the focus of this application.
pub fn draw(app: &mut App, context: &egui::Context) {
    hold_input(app, context);
    let standing = app.menu.is_open();
    let chosen = app.menu.show(context);
    if standing && !app.menu.is_open() && app.search.open {
        app.search.focus_wanted = true;
    }
    let Some(chosen) = chosen else {
        return;
    };
    let id = chosen.id;

    if let Some(source) = id
        .strip_prefix(crate::ui::connect::SOURCE_DEFAULT)
        .and_then(|_| crate::ui::connect::source_of(app, &id))
    {
        // The menu closes on a choice, and a mark nobody saw says nothing, so
        // the list comes back with the source now carrying it.
        app.set_default_source(source);
        app.open_source_menu();
        return;
    }
    if id.starts_with(crate::ui::connect::SOURCE) {
        match crate::ui::connect::source_of(app, &id) {
            Some(source) => app.open_source(source),
            // A device that was pulled out between the list being drawn and an
            // entry of it being chosen is a device there is no path to open.
            None => log::debug!("source menu: {id} names nothing to open"),
        }
        return;
    }
    if id.starts_with(crate::ui::choice::CHOICE) {
        crate::ui::choice::apply(app, &id);
        return;
    }
    if let Some(slug) = id.strip_prefix(SEARCH_KIND) {
        if let Some(kind) = crate::search::kind_of_slug(slug) {
            app.set_search_kind(kind);
        }
        return;
    }
    if crate::ui::history::chosen(app, &id, chosen.held.shift) {
        return;
    }
    if let Some(name) = id.strip_prefix(PROFILE) {
        app.set_transfer_profile(name.to_string());
        if app.dropped.is_some() {
            app.open_drop_menu();
        }
        return;
    }
    if id == AppCommand::SendFile.id()
        && let Some(path) = app.take_dropped()
    {
        app.start_picked_transfer(zyt_xfer::Direction::Send, std::slice::from_ref(&path));
        return;
    }
    if id == CONNECTION_SETTINGS {
        app.open_connection_settings();
        return;
    }
    if id.starts_with(LOST) {
        lost_action(app, &id, context);
        return;
    }
    if id.starts_with("link.") {
        link_action(app, &id, context);
        return;
    }
    if id == "drop.type_path" {
        if let Some(path) = app.take_dropped() {
            app.type_path(&path);
        }
        return;
    }
    if id == "drop.insert_contents" {
        if let Some(path) = app.take_dropped() {
            app.insert_file_contents(&path);
        }
        return;
    }
    if id == SAVE_SELECTION || id == SAVE_SELECTION_AS {
        app.save_selection(id == SAVE_SELECTION_AS);
        return;
    }
    if id == REMEMBER_SELECTION {
        app.remember_selection();
        return;
    }
    if id == "terminal.pick_file" {
        app.pick_path_to_type(false);
        return;
    }
    if id == "terminal.pick_directory" {
        app.pick_path_to_type(true);
        return;
    }
    if id.starts_with("file.") {
        file_action(app, &id);
        return;
    }
    if let Some(command) = AppCommand::from_id(&zyt_keymux::CommandId::new(&id)) {
        app.run_command(command, context);
    }
}

/// The three ways out of a source the window cannot reach.
///
/// Opening it again is the first of them, which is what a device that was
/// unplugged for a moment wants; the picker is second, because another source
/// may be the one that was meant; and a window with nothing left to show may
/// simply be done.
pub fn lost_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new("lost.again", t!("lost.again")),
        MenuItem::new("lost.another", t!("lost.another")),
        MenuItem::new("lost.close", t!("lost.close")),
    ]
}

/// Acts on the way out that was chosen.
///
/// The question is answered either way, so it is taken before anything is done
/// about it: opening the source again may fail, and then it is a new question
/// with a message of its own.
///
/// Choosing another source is `App::disconnect`, the same code the status bar
/// and the binding reach: the source is let go of, the settings are told there
/// is none, and the picker comes up. A window that came back from this question
/// must not start on a source it was told to leave.
fn lost_action(app: &mut App, id: &str, context: &egui::Context) {
    let Some(lost) = app.lost.take() else {
        return;
    };
    match id {
        "lost.again" => app.connect_source(lost.source),
        "lost.close" => context.send_viewport_cmd(egui::ViewportCommand::Close),
        _ => app.disconnect(),
    }
}

/// Keeps the pointer and the keyboard with the menu while it stands.
///
/// The widget draws plates and reads the keys it walks by; what happens to the
/// window behind it is not its business, and it is this one's. Two things are
/// said here for as long as a menu is open: nothing below its layer may be
/// interacted with, so a click beside the menu closes it and does nothing else —
/// it used to press the button it landed on as well — and no widget keeps the
/// keyboard, so `Enter` on a button of the bar behind cannot fire while `Enter`
/// chooses an entry. Which contexts the bindings are resolved in is settled in
/// `App::settle_contexts`, and while a menu stands they are the menu's alone.
fn hold_input(app: &App, context: &egui::Context) {
    if !app.menu.is_open() {
        return;
    }

    let layer = app.menu.layer_id();
    context.memory_mut(|memory| {
        memory.set_modal_layer(layer);
        if let Some(focused) = memory.focused() {
            memory.surrender_focus(focused);
        }
    });
}

/// Entries of the context menu of the terminal.
///
/// What the terminal holds decides what is offered. Copying asks for a
/// selection, so without one the entry is there but cannot be chosen, and says
/// why. With one, the two ways of saving it stand beside the copy — into a file,
/// and into a file the desktop is then asked to open — because the clipboard is
/// not the only place a page of output is wanted in.
///
/// With a selection the rest of the menu is not there at all. The entries that
/// write into the terminal — the paste and the two that type a path — would put
/// the caret somewhere else, and the entries of a transfer are about a file of a
/// machine and not about what is on the screen. A menu opened on a selection was
/// opened to do something with that selection. A transfer that is running is
/// stopped from the panel of what runs, from the menu of the session and from the
/// palette, so nothing is out of reach.
///
/// The settings stand at the end, below a line of their own. They are about the
/// window and not about the terminal, which is why nothing else about the
/// window is here — but this is the menu the pointer opens where it already is,
/// and walking to the bar to reach the one thing every other menu leads to is a
/// walk nobody should have to make.
pub fn terminal_items(app: &App) -> Vec<MenuItem> {
    let selected = app
        .session
        .terminal
        .selected_text()
        .is_some_and(|text| !text.trim().is_empty());

    let copy = command_item(app, AppCommand::Copy)
        .enabled(selected)
        .hint(if selected {
            t!("menu.copy_hint")
        } else {
            t!("menu.no_selection")
        });

    let mut items = vec![copy];
    if selected {
        items.extend(selection_items());
        items.push(MenuItem::separator());
        items.push(remember_selection_item(app));
    } else {
        items.extend(paste_items(app));
        items.push(MenuItem::separator());
        items.extend(transfer_items(app));

        if app.session.is_transferring() {
            items.push(MenuItem::separator());
            items.push(command_item(app, AppCommand::CancelTransfer));
        }
    }

    items.push(MenuItem::separator());
    items.push(command_item(app, AppCommand::SettingsOpen));

    items
}

/// Entries of the menu of the session, which the name of the source in the
/// status bar opens.
///
/// Pressing that name used to end the connection outright, which is a thing that
/// cannot be taken back done by the control a pointer lands on first. It opens
/// this instead: what there is to do with the session, ending it included, and
/// the way out of the window below a line of its own. The three entries that
/// write into it stand between the screen and the transfer, the same three the
/// menu of the terminal offers, because reaching them by the pointer should not
/// depend on which of the two menus was opened.
pub fn session_items(app: &App) -> Vec<MenuItem> {
    let mut items = vec![command_item(app, AppCommand::Clear), MenuItem::separator()];
    items.extend(paste_items(app));
    items.push(MenuItem::separator());
    items.extend(transfer_items(app));
    items.push(MenuItem::separator());
    items.push(command_item(app, AppCommand::PaletteOpen));
    items.push(connection_settings_item(app));
    items.push(MenuItem::separator());
    items.push(command_item(app, AppCommand::PortDisconnect));
    items.push(command_item(app, AppCommand::Quit));
    items
}

/// The two entries that put a selection into a file.
///
/// Both ask the dialog where it lands, and the second hands the file to the
/// chooser of the desktop once it is there: a page of a log is read in whatever
/// reads a page of a log best, and picking the program is the same question
/// `link.open_with` asks of a link.
///
/// They carry no shortcut, because they are not commands: the selection they are
/// about is made with the pointer, and the menu the pointer opens is where they
/// belong.
fn selection_items() -> Vec<MenuItem> {
    vec![
        MenuItem::new(SAVE_SELECTION, t!("selection.save")).hint(t!("selection.save_hint")),
        MenuItem::new(SAVE_SELECTION_AS, t!("selection.save_open_with"))
            .hint(t!("selection.save_open_with_hint")),
    ]
}

/// The entry that puts the selection among the commands added by hand.
///
/// It stands in a section of its own: the two above it are about a file, this
/// one is about the list the history button opens, and a selection is put there
/// to be typed back into this source or into another one — the list is shared by
/// all of them.
///
/// A history that keeps nothing is the one case where it is there and cannot be
/// chosen: the setting says how many commands are kept, and none means there is
/// nowhere to add one. It says that instead of disappearing, so the way to the
/// list is in the same place whatever the settings say.
fn remember_selection_item(app: &App) -> MenuItem {
    let kept = app.settings.command_history > 0;
    MenuItem::new(REMEMBER_SELECTION, t!("selection.remember"))
        .enabled(kept)
        .hint(if kept {
            t!("selection.remember_hint")
        } else {
            t!("selection.remember_none")
        })
}

/// The entries that write into the session: the clipboard, and a path picked
/// from the file dialog.
///
/// They are the same three in the menu of the terminal and in the menu of the
/// session, because they do the same thing in both: what they write goes where
/// the caret stands, whichever menu it was asked for from.
fn paste_items(app: &App) -> Vec<MenuItem> {
    vec![
        command_item(app, AppCommand::Paste),
        MenuItem::new("terminal.pick_file", t!("menu.pick_file")).hint(t!("menu.pick_hint")),
        MenuItem::new("terminal.pick_directory", t!("menu.pick_directory"))
            .hint(t!("menu.pick_hint")),
    ]
}

/// Entries of the menu of the link under the pointer.
///
/// A `file://` address of a trusted session names something of this machine, and
/// then the menu is that thing's own: the menu of a file, or the menu of a
/// directory, with nothing of the terminal mixed into it. Every other address
/// is only opened and copied, with the terminal entries below it.
pub fn link_items(app: &App) -> Vec<MenuItem> {
    match safe_path(app) {
        Some(path) if path.is_dir() => directory_items(app, &path),
        Some(path) => file_items(app, &path),
        None => plain_link_items(app),
    }
}

/// Entries of a link that names nothing of this machine: it is opened and
/// copied, and the terminal entries stand below it.
///
/// A `file://` address of a session that is not trusted is such a link — but it
/// is not opened: the path is the word of that session, and opening it would
/// act on a file of this machine on its say-so.
fn plain_link_items(app: &App) -> Vec<MenuItem> {
    let names_a_path = app
        .link_menu
        .as_ref()
        .is_some_and(|target| zyt_files::path_of(&target.uri).is_some());

    let mut items = link_title(app);
    if !names_a_path {
        items.push(MenuItem::new("link.open", t!("link.open")));
        items.push(MenuItem::new("link.open_with", t!("link.open_with")));
    }
    items.push(MenuItem::new("link.copy_link", t!("link.copy_link")));
    items.push(MenuItem::new("link.copy_text", t!("link.copy_text")));
    items.push(MenuItem::separator());
    items.extend(terminal_items(app));
    items
}

/// File or directory the link names, when the session is trusted and it is
/// there.
///
/// A path is only a path because the session that printed it is believed: an
/// untrusted one could name any file of this machine, so the operations are
/// offered for a safe session only.
pub fn safe_path(app: &App) -> Option<std::path::PathBuf> {
    if !app.session_is_trusted() {
        return None;
    }
    let target = app.link_menu.as_ref()?;
    let path = zyt_files::path_of(&target.uri)?;
    path.exists().then_some(path)
}

/// Longest a name is shown as on the plate that titles a menu.
const TITLE_LENGTH: usize = 16;

/// The plate a link menu opens with: what the menu is about.
///
/// It is there to be read, not chosen — a menu of eight entries on a path says
/// nothing about which path. An address of a file is named by the file, every
/// other by its domain, and the whole address is the hint.
fn link_title(app: &App) -> Vec<MenuItem> {
    let Some(target) = app.link_menu.as_ref() else {
        return Vec::new();
    };

    let label = match zyt_files::path_of(&target.uri) {
        Some(path) => shorten(&file_name(&path), TITLE_LENGTH),
        None => match domain_of(&target.uri) {
            Some(domain) => domain,
            None => shorten(&target.uri, TITLE_LENGTH),
        },
    };

    vec![
        MenuItem::new("link.title", label)
            .enabled(false)
            .hint(target.uri.clone()),
        MenuItem::separator(),
    ]
}

/// Name of the file a path ends with, the path itself when it has none.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Text of at most this many characters, what was cut marked with an ellipsis.
pub fn shorten(text: &str, length: usize) -> String {
    if text.chars().count() <= length {
        return text.to_string();
    }
    let kept: String = text.chars().take(length).collect();
    format!("{kept}{}", crate::ui::icons::ELLIPSIS)
}

/// Domain an address names, without what stands before or after it.
fn domain_of(uri: &str) -> Option<String> {
    let rest = uri.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = match host.rsplit_once(':') {
        Some((before, port)) if port.chars().all(|digit| digit.is_ascii_digit()) => before,
        _ => host,
    };
    (!host.is_empty()).then(|| host.to_string())
}

/// What can be done with a file of this machine.
fn file_items(app: &App, path: &std::path::Path) -> Vec<MenuItem> {
    let menu = &app.settings.file_menu;
    let mut items = link_title(app);

    if menu.open {
        items.push(MenuItem::new("link.open", t!("link.open")));
    }
    if menu.open_with {
        items.push(MenuItem::new("link.open_with", t!("link.open_with")));
    }
    if menu.rename {
        items.push(MenuItem::new("file.rename", t!("file.rename")).hint(t!("file.rename_hint")));
    }
    if menu.move_here
        && let Some(item) = move_here_item(app, path)
    {
        items.push(item);
    }
    if menu.copy_link {
        items.push(MenuItem::new("link.copy_link", t!("link.copy_link")));
    }
    if menu.copy_text {
        items.push(MenuItem::new("link.copy_text", t!("link.copy_text")));
    }
    if menu.copy_contents {
        items.push(copy_contents_item(app, path));
    }
    if menu.trash {
        items.push(MenuItem::new("file.trash", t!("file.trash")));
    }
    if menu.delete {
        items.push(MenuItem::new("file.delete", t!("file.delete")));
    }

    items
}

/// What can be done with a directory of this machine.
fn directory_items(app: &App, path: &std::path::Path) -> Vec<MenuItem> {
    let menu = &app.settings.directory_menu;
    let mut items = link_title(app);

    if menu.open {
        items.push(MenuItem::new("link.open", t!("directory.open")));
    }
    if menu.open_with {
        items.push(MenuItem::new("link.open_with", t!("link.open_with")));
    }
    if menu.rename {
        items.push(
            MenuItem::new("file.rename", t!("file.rename")).hint(t!("directory.rename_hint")),
        );
    }
    if menu.move_here
        && let Some(item) = move_here_item(app, path)
    {
        items.push(item);
    }
    if menu.copy_link {
        items.push(MenuItem::new("link.copy_link", t!("link.copy_link")));
    }
    if menu.copy_text {
        items.push(MenuItem::new("link.copy_text", t!("link.copy_text")));
    }
    if menu.trash {
        items.push(MenuItem::new("file.trash", t!("directory.trash")));
    }
    if menu.delete {
        items.push(
            MenuItem::new("file.delete", t!("directory.delete")).hint(t!("directory.delete_hint")),
        );
    }

    items
}

/// Moving into the directory of the session, offered while that is another one.
fn move_here_item(app: &App, path: &std::path::Path) -> Option<MenuItem> {
    let directory = app.working_directory()?;
    if path.parent() == Some(directory.as_path()) || path == directory {
        return None;
    }
    Some(
        MenuItem::new("file.move_here", t!("file.move_here"))
            .detail(directory.display().to_string()),
    )
}

/// Copying what a file holds, offered only while a clipboard could carry it.
fn copy_contents_item(app: &App, path: &std::path::Path) -> MenuItem {
    contents_item(app, path, "file.copy_contents", false)
}

/// An entry that reads a file whole: offered while the file is small enough and
/// its content is of a kind that can be carried at all. A terminal takes text
/// alone; a clipboard takes a picture as well.
///
/// What cannot be carried says so in its hint instead of disappearing, because
/// a menu that changes shape with every file is a menu that has to be read
/// again every time.
fn contents_item(app: &App, path: &std::path::Path, id: &str, text_only: bool) -> MenuItem {
    let limit = app.settings.file_menu.copy_limit;
    let size = zyt_files::size_of(path).unwrap_or_default();
    let fits = size <= limit;
    let content = zyt_files::content_of(path);
    let carried = if text_only {
        matches!(content, zyt_files::Content::Text(_))
    } else {
        !matches!(content, zyt_files::Content::Other(_))
    };

    MenuItem::new(id, t!(id))
        .detail(crate::format::size(size))
        .hint(if !carried {
            t!("file.not_copied", media = content.media_type())
        } else if fits {
            t!("file.copy_contents_hint", media = content.media_type())
        } else {
            t!("file.too_large", limit = crate::format::size(limit))
        })
        .enabled(fits && carried)
}

/// Entries of the transfer control: the two directions and the profile they use,
/// which carries every profile as entries of its own.
pub fn transfer_items(app: &App) -> Vec<MenuItem> {
    let mut items = Vec::new();
    for command in [AppCommand::SendFile, AppCommand::ReceiveFile] {
        if is_offered(app, command) {
            items.push(command_item(app, command));
        }
    }
    items.push(profile_item(app));
    items
}

/// Entries offered for a file dropped onto the window: send it, and with which
/// profile. A file that arrives is a file to send, so there is no direction to
/// choose.
pub fn drop_items(app: &App) -> Vec<MenuItem> {
    let Some(path) = app.dropped.clone() else {
        return Vec::new();
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    vec![
        command_item(app, AppCommand::SendFile)
            .detail(name.clone())
            .enabled(is_offered(app, AppCommand::SendFile)),
        MenuItem::new("drop.type_path", t!("drop.type_path"))
            .detail(name)
            .hint(t!("drop.type_path_hint")),
        contents_item(app, &path, "drop.insert_contents", true),
        profile_item(app),
    ]
}

/// Entries of the command palette: every command of the current contexts.
pub fn palette_items(app: &mut App) -> Vec<MenuItem> {
    app.palette_hits()
        .into_iter()
        .map(|hit| {
            let keys = hit
                .keys
                .as_ref()
                .map(|chord| chord.to_text())
                .unwrap_or_default();
            MenuItem::new(hit.command.id.as_str(), hit.command.title.clone()).detail(keys)
        })
        .collect()
}

/// The plate naming the profile a transfer runs with, carrying every profile as
/// entries of its own.
///
/// The one in use is marked and can be chosen like the rest: picking it again
/// changes nothing, which is what someone who opened the menu and changed their
/// mind wants.
fn profile_item(app: &App) -> MenuItem {
    let current = app
        .active_profile()
        .map(|profile| profile.name.clone())
        .unwrap_or_default();

    let profiles: Vec<MenuItem> = app
        .offered_profiles()
        .into_iter()
        .map(|profile| {
            let mark = if profile.name == current {
                crate::ui::icons::CURRENT
            } else {
                ""
            };
            MenuItem::new(format!("{PROFILE}{}", profile.name), &profile.name).detail(mark)
        })
        .collect();

    MenuItem::new("transfer.profile", t!("menu.profile", name = current))
        .opens_at(format!("{PROFILE}{current}"))
        .hint(t!("settings.transfer_profiles"))
        .enabled(false)
        .children(profiles)
}

/// The entry of the way a query is read that is in use, which is where its
/// menu opens.
pub fn current_search_kind(app: &App) -> String {
    format!(
        "{SEARCH_KIND}{}",
        crate::search::kind_slug(app.search.options.kind)
    )
}

/// Every way of reading a search query, the one in use marked.
///
/// It can be chosen like any other: picking it again searches again with it,
/// which is what someone who opened the menu and changed their mind wants.
pub fn search_kind_items(app: &App) -> Vec<MenuItem> {
    let current = app.search.options.kind;
    zyt_term::SearchKind::ALL
        .into_iter()
        .map(|kind| {
            let key = crate::search::kind_key(kind);
            let mark = if kind == current {
                crate::ui::icons::CURRENT
            } else {
                ""
            };
            MenuItem::new(
                format!("{SEARCH_KIND}{}", crate::search::kind_slug(kind)),
                t!(key),
            )
            .hint(t!(format!("{key}_hint")))
            .detail(mark)
        })
        .collect()
}

/// A transfer the active profile cannot run is left out of the menu.
fn is_offered(app: &App, command: AppCommand) -> bool {
    let Some(direction) = transfer_direction(command) else {
        return true;
    };
    app.active_profile()
        .is_some_and(|profile| profile.commands(direction).is_available())
}

/// One entry for a command: its title and the keys that run it.
/// The way into the settings from the menu of the session, which is the half of
/// them about this connection.
///
/// It is the same entry everywhere else opens the settings with, and it says the
/// same key, because it is the same place. Which half it opens on is what the
/// menu it stands in says: this one is the menu of the source the window is on,
/// and somebody walking into the settings from it is walking there about that
/// source.
fn connection_settings_item(app: &App) -> MenuItem {
    let command = AppCommand::SettingsOpen;
    let key = command.title_key();
    MenuItem::new(CONNECTION_SETTINGS, t!(&key)).detail(shortcut_or_empty(&app.dispatcher, command))
}

fn command_item(app: &App, command: AppCommand) -> MenuItem {
    let key = command.title_key();
    MenuItem::new(command.id(), t!(&key)).detail(shortcut_or_empty(&app.dispatcher, command))
}

/// Direction a transfer command works in, if it is one.
fn transfer_direction(command: AppCommand) -> Option<zyt_xfer::Direction> {
    match command {
        AppCommand::SendFile => Some(zyt_xfer::Direction::Send),
        AppCommand::ReceiveFile => Some(zyt_xfer::Direction::Receive),
        _ => None,
    }
}

/// What one of the file entries does with the file the menu was opened for.
fn file_action(app: &mut App, id: &str) {
    let Some(path) = safe_path(app) else {
        return;
    };
    match id {
        "file.rename" => app.rename_file(&path),
        "file.move_here" => app.move_file_here(&path),
        "file.copy_contents" => app.copy_file_contents(&path),
        "file.trash" => app.trash_file(&path),
        "file.delete" => app.delete_file(&path),
        _ => {}
    }
    app.link_menu = None;
}

/// What one of the link entries does with the link the menu was opened for.
fn link_action(app: &mut App, id: &str, context: &egui::Context) {
    let Some(target) = app.link_menu.clone() else {
        return;
    };
    match id {
        "link.open" => app.open_link(&target.uri),
        "link.open_with" => app.open_link_with(&target.uri),
        "link.copy_link" => context.copy_text(copied_link(&target.uri)),
        "link.copy_text" => context.copy_text(target.text.clone()),
        _ => {}
    }
    app.link_menu = None;
}

/// What the clipboard is given for the address of a link.
///
/// A `file://` address names a place of this machine, and a place is written as
/// a path everywhere else: in a shell, in an editor, in a message. So the
/// scheme is dropped and the escapes are resolved, and what is copied is the
/// path itself. Every other address is copied as it stands.
fn copied_link(uri: &str) -> String {
    match zyt_files::path_of(uri) {
        Some(path) => path.to_string_lossy().into_owned(),
        None => uri.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_address_is_copied_as_its_path() {
        assert_eq!(copied_link("file:///srv/notes.txt"), "/srv/notes.txt");
        assert_eq!(
            copied_link("file:///srv/two%20words/notes.txt"),
            "/srv/two words/notes.txt"
        );
        assert_eq!(copied_link("file://localhost/etc/hosts"), "/etc/hosts");
    }

    #[test]
    fn an_address_of_elsewhere_is_copied_whole() {
        assert_eq!(
            copied_link("https://example.com/a"),
            "https://example.com/a"
        );
        assert_eq!(
            copied_link("file://other-host/etc/hosts"),
            "file://other-host/etc/hosts"
        );
    }

    #[test]
    fn a_long_name_is_cut_and_marked() {
        assert_eq!(shorten("notes.txt", 16), "notes.txt");
        assert_eq!(shorten("0123456789abcdef", 16), "0123456789abcdef");
        assert_eq!(shorten("0123456789abcdefg", 16), "0123456789abcdef\u{2026}");
        assert_eq!(
            shorten("отчёт-за-январь-2026.txt", 8),
            "отчёт-за\u{2026}",
            "counted in characters, not in bytes"
        );
    }

    #[test]
    fn an_address_is_named_by_its_domain() {
        assert_eq!(
            domain_of("https://example.com/a/b?c#d"),
            Some("example.com".to_string())
        );
        assert_eq!(
            domain_of("https://user:secret@example.com:8443/a"),
            Some("example.com".to_string())
        );
        assert_eq!(domain_of("file:///srv/notes.txt"), None);
        assert_eq!(domain_of("mailto:someone@example.com"), None);
    }
}
