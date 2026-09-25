//! The sources this window can open, as entries of the menu of plates.
//!
//! A list to choose from is one widget in this program, and the sources are one
//! list like any other: the ports of the machine and the consoles that are set
//! up, newest connection first. What an entry says is what tells two devices of
//! one kind apart — the path or the program on the plate, the name of the
//! device beside it, and the moment it was last opened on the plate of its own.

use crate::app::App;
use crate::config::SourceSetting;
use crate::sources::SourceKey;
use crate::ui::icons;
use plate_menu::MenuItem;
use rust_i18n::t;
use zyt_serial::PortInfo;

/// Prefix of an entry that names a source to open.
pub const SOURCE: &str = "source:";
/// Prefix of the entry that makes a source the one a window starts on.
pub const SOURCE_DEFAULT: &str = "source.default:";

/// One line of the list: a serial port or a console.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    /// Key this source is addressed by.
    key: SourceKey,
    /// Name of the device, shown beside the path.
    name: String,
    /// Path or program that identifies the source.
    path: String,
}

impl Entry {
    /// What the settings call this source when they name the one a window
    /// opens.
    ///
    /// A console is named by what it is; a port by the path it is opened at,
    /// because the identity of a device is not something anybody can ask for
    /// before it has been found.
    fn source(&self) -> SourceSetting {
        match self.key.console() {
            Some(id) => SourceSetting::Console { id },
            None => SourceSetting::Port {
                path: self.path.clone(),
            },
        }
    }

    /// What an entry of this source is called in the menu.
    fn id(&self) -> String {
        format!("{SOURCE}{}", self.key)
    }
}

/// Every source as an entry of the menu, the one opened last at the top.
///
/// An entry is chosen and stepped into both: choosing it opens the source,
/// which is what a list of sources is walked for, and the level below it holds
/// what else there is to say about it — whether a window starts on it. That way
/// the common thing is one key and the rare one is one more.
pub fn items(app: &App) -> Vec<MenuItem> {
    let mut entries = collect(app);
    sort(&mut entries, |key| {
        app.memory(key).and_then(|memory| memory.last_connected)
    });

    entries.iter().map(|entry| item(app, entry)).collect()
}

/// The source an entry of the menu is about, whichever of the two levels it is
/// on.
pub fn key_of(id: &str) -> Option<SourceKey> {
    id.strip_prefix(SOURCE_DEFAULT)
        .or_else(|| id.strip_prefix(SOURCE))?
        .parse()
        .ok()
}

/// The source an entry of the menu names, whichever of the two levels it is on.
///
/// A device is named by the path it is opened at and not by the identity it is
/// remembered under, because that identity is what the scan found and a window
/// opens a path.
pub fn source_of(app: &App, id: &str) -> Option<SourceSetting> {
    let key = key_of(id)?;

    if let Some(id) = key.console() {
        return Some(SourceSetting::Console { id });
    }
    let port = key.port()?;
    let path = app
        .ports
        .iter()
        .find(|found| found.id() == *port)
        .map(|found| found.path.clone())?;
    Some(SourceSetting::Port { path })
}

/// The entry of one source: the path on the plate, the device beside it, the
/// times on the plate of its own, and the level below it.
///
/// Typing finds it by either of the two: a board is remembered as the path it
/// is at by whoever set it up and as the name of the thing plugged in there by
/// whoever plugged it in, and the one looking for it types whichever they have
/// in mind.
fn item(app: &App, entry: &Entry) -> MenuItem {
    let current = app.settings.default_source == entry.source();
    let mark = if current { icons::CURRENT } else { "" };
    let hint = if current {
        t!("ports.default_source_drop")
    } else {
        t!("ports.default_source_set")
    };

    let default = MenuItem::new(
        format!(
            "{SOURCE_DEFAULT}{}",
            entry.id().strip_prefix(SOURCE).unwrap_or_default()
        ),
        t!("ports.default_source"),
    )
    .detail(mark)
    .hint(hint);

    MenuItem::new(entry.id(), &entry.path)
        .detail(&entry.name)
        .search(&entry.name)
        .full(whole(app, entry))
        .choosable(true)
        .children(vec![default])
}

/// What the plate beside the menu says: the source, the moment it was last
/// opened and, for a port, the line it was opened with.
///
/// Every source carries one, whether anything was ever connected to it or not:
/// a plate that comes and goes as the selection moves says less than one that
/// is always there to be read.
fn whole(app: &App, entry: &Entry) -> String {
    let memory = app.memory(&entry.key);
    let time = memory
        .and_then(|memory| memory.last_connected)
        .map(crate::format::time)
        .unwrap_or_else(|| t!("ports.never").to_string());

    let mut text = format!("{}\n\n{} {time}", entry.path, icons::HISTORY);
    if let Some(memory) = entry.key.port().and_then(|id| app.ports_memory.get(id)) {
        text.push_str(&format!("\n{}", memory.line.summary()));
    }
    text
}

/// Every source the user can pick: the ports of the system and the consoles,
/// which are entries like any other.
fn collect(app: &App) -> Vec<Entry> {
    let mut entries: Vec<Entry> = app
        .ports
        .iter()
        .map(|port: &PortInfo| Entry {
            key: SourceKey::Port(port.id()),
            name: port.resolved_name(),
            path: port.path.clone(),
        })
        .collect();

    for console in &app.consoles {
        entries.push(Entry {
            key: console.key(),
            name: console.display_name(),
            path: console.program_or_shell(),
        });
    }
    entries
}

/// Orders the sources by the time they were used last, newest first, and by
/// path for sources that were never used.
fn sort(entries: &mut [Entry], last_connected: impl Fn(&SourceKey) -> Option<jiff::Timestamp>) {
    entries.sort_by(|left, right| {
        let left_time = last_connected(&left.key);
        let right_time = last_connected(&right.key);
        right_time
            .cmp(&left_time)
            .then_with(|| left.path.cmp(&right.path))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::consoles::ConsoleId;

    fn port(path: &str) -> Entry {
        Entry {
            key: SourceKey::Port(path.parse().expect("the identity reads")),
            name: "device".to_string(),
            path: path.strip_prefix("path:").unwrap_or(path).to_string(),
        }
    }

    fn console(id: ConsoleId) -> Entry {
        Entry {
            key: SourceKey::Console(id),
            name: "shell".to_string(),
            path: "/bin/sh".to_string(),
        }
    }

    #[test]
    fn a_console_is_sorted_like_every_other_source() {
        let shell = ConsoleId::new();
        let times: BTreeMap<SourceKey, jiff::Timestamp> = [
            (SourceKey::Console(shell), "2026-09-10T10:00:00Z"),
            (
                SourceKey::Port("path:/dev/ttyS1".parse().expect("it reads")),
                "2026-09-01T10:00:00Z",
            ),
        ]
        .into_iter()
        .map(|(key, time)| (key, time.parse().expect("timestamp parses")))
        .collect();

        let mut entries = vec![
            port("path:/dev/ttyUSB0"),
            port("path:/dev/ttyS1"),
            console(shell),
        ];

        sort(&mut entries, |key| times.get(key).copied());

        let order: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(order, vec!["/bin/sh", "/dev/ttyS1", "/dev/ttyUSB0"]);
    }

    #[test]
    fn an_entry_says_which_source_it_is_about() {
        let shell = ConsoleId::new();
        let device = port("usb:0403:6001:A1");

        assert_eq!(console(shell).id(), format!("source:{shell}"));
        assert_eq!(device.id(), "source:usb:0403:6001:A1");

        assert_eq!(
            key_of(&console(shell).id()),
            Some(SourceKey::Console(shell))
        );
        assert_eq!(key_of(&device.id()), Some(device.key.clone()));
    }

    #[test]
    fn the_entry_below_a_source_is_about_the_same_source() {
        let shell = ConsoleId::new();
        let below = format!("{SOURCE_DEFAULT}{shell}");

        assert_eq!(key_of(&below), Some(SourceKey::Console(shell)));
    }

    #[test]
    fn an_identifier_of_another_menu_is_about_no_source() {
        assert_eq!(key_of("profile:zmodem"), None);
        assert_eq!(key_of("source:what:ever"), None);
    }
}
