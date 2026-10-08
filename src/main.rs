//! Terminal for serial consoles and the local shell.
//!
//! A release build for Windows is linked against the windows subsystem, so
//! starting the program from the desktop opens no console window behind it. A
//! debug build keeps the console, where the log goes.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod answers;
mod app;
mod caret;
mod commands;
mod config;
mod consoles;
mod error;
mod fonts;
mod format;
mod forms;
mod history;
mod keys;
mod metrics;
mod openwith;
mod primary;
mod rate;
mod render;
mod scripts;
mod search;
mod session;
mod sources;
mod taskbar;
mod theme;
mod themes;
mod ui;

use crate::app::App;
use crate::config::{SETTINGS_FILE, Settings};
use crate::consoles::ConsoleId;
use crate::error::{AppError, Result};
use clap::Parser;
use std::path::{Path, PathBuf};
use zyt_config::{AppId, ConfigStore};

/// What the command line says.
#[derive(Debug, Parser)]
#[command(name = WINDOW_TITLE, version, about = "A terminal for serial ports and the local console")]
struct Arguments {
    /// Console to open, by the identity it carries.
    ///
    /// Without it the window opens the source the settings call the default
    /// one, and without that it comes up with the list of them. It is also what
    /// a window started from another window is given, so the new window opens
    /// the console the old one is on — which is why it is the identity and not
    /// the name: a console is renamed and stays the console it was.
    #[arg(long, value_name = "ID", value_parser = console_id)]
    console: Option<ConsoleId>,
    /// Where the window starts: a directory, or a file standing in one.
    ///
    /// This is what a file manager hands over when its menu is used to open a
    /// folder with this program, which is why it is a path and not an option
    /// with a name: a desktop entry writes `%f`, and a program started from the
    /// menu of the desktop gets nothing in its place.
    ///
    /// A console that names a directory of its own is still started there. This
    /// is the directory of the window, which is what a console without one
    /// follows.
    #[arg(value_name = "PATH", value_parser = start_path)]
    path: Option<PathBuf>,
}

/// Reads the identity of a console off the command line.
fn console_id(text: &str) -> std::result::Result<ConsoleId, String> {
    text.parse::<ConsoleId>()
        .map_err(|_| format!("not the identity of a console: {text}"))
}

/// Reads the place a window starts in off the command line.
///
/// A file manager hands over a plain path for `%f` and an address for `%u`, and
/// both name the same thing, so both are read.
fn start_path(text: &str) -> std::result::Result<PathBuf, String> {
    Ok(crate::openwith::local_path(text).unwrap_or_else(|| PathBuf::from(text)))
}

rust_i18n::i18n!("locales", fallback = "en");

/// Window title of the application.
pub const WINDOW_TITLE: &str = "ZYTerm";

/// Identity of the application, in reverse domain form.
///
/// It is what the desktop names the program by: the window carries it as its
/// `app_id`, and the desktop entry and the icon are installed under it. A
/// window whose `app_id` names no entry is a window the desktop cannot match to
/// the program, so it gets the default icon and a second entry of its own.
pub const APP_ID: &str = "ru.styxheim.zyterm";

/// The one name the directories of the application are resolved under.
///
/// It is the application part of [`APP_ID`], and it is what the shared
/// directories of the system carry: `/usr/share/zyterm/scripts` is where the
/// scripts of every user of this machine stand.
pub const APP_NAME: &str = "zyterm";

fn main() -> std::process::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let arguments = Arguments::parse();
    if let Some(path) = &arguments.path {
        enter_path(path);
    }

    let (store, settings) = match load_settings() {
        Ok(loaded) => loaded,
        Err(error) => {
            log::error!("cannot load settings: {error}");
            let store = fallback_store();
            shipped_consoles(&store);
            (store, Settings::default())
        }
    };
    rust_i18n::set_locale(settings.locale.code());

    match run(store, settings, arguments.console) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            log::error!("window creation failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn load_settings() -> Result<(ConfigStore, Settings)> {
    let store = ConfigStore::new(&AppId {
        qualifier: "ru".to_string(),
        organization: "styxheim".to_string(),
        application: APP_NAME.to_string(),
    })
    .map_err(AppError::from)?;

    match store.load_or_create(SETTINGS_FILE, Settings::default) {
        Ok(settings) => {
            shipped_consoles(&store);
            Ok((store, settings))
        }
        Err(zyt_config::ConfigError::Decode { .. }) => {
            let settings = start_over(&store)?;
            shipped_consoles(&store);
            Ok((store, settings))
        }
        Err(error) => Err(AppError::from(error)),
    }
}

/// Writes the consoles this program ships that the machine has not got.
///
/// A window needs somewhere to run, so a configuration directory without a
/// single console file gets them all. One that has consoles already gets the
/// ones shipped since it was last started, told apart by the identity every
/// shipped console carries on every machine. They are written rather than held
/// in memory, because a console is a file of this application and everything
/// that reads one reads it from there.
fn shipped_consoles(store: &ConfigStore) {
    for console in crate::consoles::add_shipped(store) {
        log::info!(
            "the console {} ({}) is shipped with this program and was written",
            console.name,
            console.id
        );
    }
}

/// Keeps a settings file the current version cannot read and starts from the
/// defaults, so an old format never blocks the application.
fn start_over(store: &ConfigStore) -> Result<Settings> {
    let path = store.path(SETTINGS_FILE);
    let backup = path.with_extension("yaml.bak");
    match std::fs::rename(&path, &backup) {
        Ok(()) => log::warn!(
            "settings could not be read, kept as {} and started from the defaults",
            backup.display()
        ),
        Err(error) => log::warn!("settings could not be read nor kept: {error}"),
    }

    let settings = Settings::default();
    store
        .save(SETTINGS_FILE, &settings)
        .map_err(AppError::from)?;
    Ok(settings)
}

fn fallback_store() -> ConfigStore {
    let base = std::env::temp_dir().join("zyterm");
    ConfigStore::with_paths(base.join("config"), base.join("data"), base.join("locks"))
}

/// Makes the place named on the command line the directory of the window.
///
/// A file manager names the folder its menu was used on, or the file, and both
/// mean the same thing: the window opens where that thing stands. It is the
/// directory of the process, because that is what a console without a directory
/// of its own is started in, and what every window started from this one
/// inherits.
///
/// A path that names nothing is a line in the log: a window that comes up in
/// the wrong directory is worth more than one that does not come up.
fn enter_path(path: &Path) {
    let directory = match path.is_dir() {
        true => path,
        false => match path.parent() {
            Some(parent) if parent.is_dir() => parent,
            _ => {
                log::warn!("{} names no directory to start in", path.display());
                return;
            }
        },
    };

    if let Err(error) = std::env::set_current_dir(directory) {
        log::warn!("cannot start in {}: {error}", directory.display());
    }
}

/// Icon of the window, drawn in `assets/icon.svg` and built into the program.
///
/// A window without an icon is only a window with the default one, so a file
/// that cannot be read is a line in the log and nothing more.
fn window_icon() -> Option<egui::IconData> {
    const ICON: &[u8] = include_bytes!("../assets/icon.png");

    match image::load_from_memory(ICON) {
        Ok(image) => {
            let image = image.into_rgba8();
            let (width, height) = image.dimensions();
            Some(egui::IconData {
                rgba: image.into_raw(),
                width,
                height,
            })
        }
        Err(error) => {
            log::warn!("window icon: {error}");
            None
        }
    }
}

fn run(
    store: ConfigStore,
    settings: Settings,
    console: Option<ConsoleId>,
) -> std::result::Result<(), String> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_app_id(APP_ID)
        .with_inner_size([960.0, 640.0])
        .with_min_inner_size([480.0, 320.0]);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = render::native_options(viewport);

    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |creation| {
            if let Some(state) = &creation.wgpu_render_state {
                render::log_adapter(state);
            }
            let app = App::new(&creation.egui_ctx, store, settings, console)
                .map_err(|error| format!("{error}"))?;
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| error.to_string())
}
