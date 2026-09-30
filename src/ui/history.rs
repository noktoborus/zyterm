//! The plates of the command history: what stands on them and what a choice
//! does.
//!
//! Two files fill this menu. The commands the shell of the source marked stand
//! beside the ones somebody added by hand, which every source shares, and the
//! identifier of an entry says which of the two it came from — that is the file
//! a removal touches.

use crate::app::App;
use crate::ui::menu::shorten;
use plate_menu::MenuItem;
use rust_i18n::t;

/// Prefix of an entry that names a command the shell of the source marked.
const HISTORY: &str = "history:";
/// Prefix of an entry that names a command somebody added by hand.
const ADDED: &str = "added:";
/// Prefix of the entry below a command that takes it out of its file. What
/// follows it is the whole identifier of that command, which says both which
/// command it is and which of the two lists it is kept in.
const FORGET: &str = "history.forget:";
/// How much of a command the plate of the history shows. What is longer is cut
/// there and ends in an ellipsis; the whole of it stands on the plate beside
/// the menu.
const HISTORY_LENGTH: usize = 64;

/// The commands to type back, newest first: the ones the shell of this source
/// marked and the ones somebody added by hand, in one list.
///
/// The two files are read here and not kept anywhere, so a copy of the
/// application that wrote to one of them a moment ago is in this menu too. They
/// are ordered by when each command last ran, because which of the two files a
/// command is kept in is not how anybody looks for it.
///
/// Every command carries one entry below it, the one that takes it out of its
/// file. The command itself is still what `Enter` chooses — `choosable` — and
/// the entry below is reached by `Right` and by no query: there is one of them
/// per command, they all read the same, and a flat list of hits is no place for
/// them.
pub fn items(app: &mut App) -> Vec<MenuItem> {
    let marked = app
        .command_history()
        .into_iter()
        .map(|entry| (HISTORY, entry));
    let added = app.added_commands().into_iter().map(|entry| (ADDED, entry));

    let mut commands: Vec<(&str, crate::history::Entry)> = marked.chain(added).collect();
    commands.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.at));

    commands
        .into_iter()
        .map(|(prefix, entry)| {
            let id = format!("{prefix}{}", entry.command);
            let shown = shorten(&one_line(&entry.command), HISTORY_LENGTH);
            MenuItem::new(&id, shown)
                .full(whole(&entry))
                .choosable(true)
                .children(vec![
                    MenuItem::new(format!("{FORGET}{id}"), t!("history.forget"))
                        .hint(t!("history.forget_hint"))
                        .searchable(false),
                ])
        })
        .collect()
}

/// Acts on an entry of the history: types the command back, or takes it out of
/// its file. False while the identifier names no entry of this menu.
///
/// The two ways of choosing a command are the two things there are to do with
/// one, and which is which is a setting: a command is typed in to be run, or to
/// be read once more and changed first.
pub fn chosen(app: &mut App, id: &str, shift: bool) -> bool {
    if let Some(entry) = id.strip_prefix(FORGET) {
        forget(app, entry);
        return true;
    }
    let Some(command) = command_of(id) else {
        return false;
    };
    let action = app.settings.command_history_keys.action(shift);
    app.run_from_history(command, action);
    true
}

/// The command an entry of the history names, whichever list it is kept in.
fn command_of(id: &str) -> Option<String> {
    let command = id
        .strip_prefix(HISTORY)
        .or_else(|| id.strip_prefix(ADDED))?;
    Some(command.to_string())
}

/// The list an entry of the history is kept in, nothing for an identifier that
/// names no list or a source the window is no longer on.
fn list_of(app: &App, id: &str) -> Option<crate::history::List> {
    if id.starts_with(ADDED) {
        return Some(crate::history::List::Added);
    }
    if id.starts_with(HISTORY) {
        return app.source_commands();
    }
    None
}

/// Takes one command out of the file it is kept in and shows what is left.
///
/// The menu closes on a choice, and a list that lost an entry is a list to look
/// at again: several commands are often taken out one after another, and walking
/// back into the history for each of them is the walk this saves. A history with
/// nothing left in it opens nothing, the way it does when nobody has written to
/// it.
fn forget(app: &mut App, entry: &str) {
    let Some(list) = list_of(app, entry) else {
        log::debug!("command history: {entry} names no list to take it out of");
        return;
    };
    let Some(command) = command_of(entry) else {
        return;
    };
    app.forget_command(&list, &command);
    app.open_history_menu();
}

/// What the plate beside the history says: the command as it was typed, the
/// directory it ran in and the moment it last ran.
///
/// Every command carries one, whether it was cut or not: a plate that comes
/// and goes as the selection moves says less than one that is always there to
/// be read, and the two lines below the command are never on the plate of the
/// list.
fn whole(entry: &crate::history::Entry) -> String {
    let mut text = entry.command.clone();
    if !entry.directory.is_empty() {
        text.push_str(&format!(
            "\n\n{} {}",
            crate::ui::icons::FOLDER,
            entry.directory
        ));
    }
    text.push_str(&format!(
        "\n{} {}",
        crate::ui::icons::HISTORY,
        crate::format::time(entry.at)
    ));
    text
}

/// A command written as one line, whatever it was typed as.
///
/// A plate is one line, and a command of several is one entry of the list: the
/// breaks become spaces, so a command that was typed over three lines takes one
/// plate and the plates below it stay where they are. What it really is stands
/// on the plate beside the menu.
fn one_line(command: &str) -> String {
    command
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_of_several_lines_is_one_plate() {
        assert_eq!(one_line("make -j4"), "make -j4");
        assert_eq!(
            one_line("for f in *; do\n  echo $f\ndone"),
            "for f in *; do echo $f done"
        );
        assert_eq!(one_line("a\r\n\r\nb"), "a b");
    }

    #[test]
    fn a_command_too_long_for_its_plate_is_cut_at_the_limit() {
        let command = "x".repeat(HISTORY_LENGTH + 10);
        let shown = shorten(&one_line(&command), HISTORY_LENGTH);

        assert_eq!(shown.chars().count(), HISTORY_LENGTH + 1, "the ellipsis");
        assert!(shown.ends_with('\u{2026}'));
    }

    /// The identifier of an entry says which command it is and which of the two
    /// files it is kept in, and the entry that removes it carries that whole
    /// identifier — so the removal knows the file as well as the command.
    #[test]
    fn an_entry_of_the_history_says_which_command_it_is() {
        assert_eq!(command_of("history:make -j4"), Some("make -j4".to_string()));
        assert_eq!(command_of("added:make -j4"), Some("make -j4".to_string()));
        assert_eq!(command_of("terminal.copy"), None);

        let forget = format!("{FORGET}{ADDED}make -j4");
        assert_eq!(
            command_of(&forget),
            None,
            "the entry that removes a command is not a command to type back"
        );
        assert_eq!(
            forget.strip_prefix(FORGET).and_then(command_of).as_deref(),
            Some("make -j4")
        );
    }
}
