//! Terminal palettes the user can choose from.
//!
//! Two palettes are built in, the dark and the light one of the widget. The
//! rest are files in the `themes` directory of the configuration, written in
//! the color format of Alacritty, which is what the themes published on the
//! network use.

use crate::error::{AppError, Result};
use egui::Color32;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use zyt_config::ConfigStore;
use zyt_term_egui::TerminalTheme;

/// Directory holding the theme files, below the configuration directory.
pub const THEMES_DIR: &str = "themes";

/// Name of the palette used for the dark mode when nothing else is chosen.
pub const BUILT_IN_DARK: &str = "Built-in dark";
/// Name of the palette used for the light mode when nothing else is chosen.
pub const BUILT_IN_LIGHT: &str = "Built-in light";

/// One palette, named after the file it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Name shown in the settings; the file name without its suffix.
    pub name: String,
    /// The colors themselves.
    pub colors: TerminalTheme,
}

/// Every palette the application can offer.
#[derive(Debug, Clone)]
pub struct ThemeCatalog {
    themes: Vec<Theme>,
    failed: Vec<String>,
}

impl ThemeCatalog {
    /// Reads the theme directory, the two built-in palettes first.
    ///
    /// A file that cannot be read or understood is left out and named in
    /// [`ThemeCatalog::failed`], so one broken file never hides the rest.
    pub fn load(store: &ConfigStore) -> Self {
        Self::from_directory(&store.path(THEMES_DIR))
    }

    /// Reads one directory of theme files.
    pub fn from_directory(directory: &Path) -> Self {
        let mut themes = vec![
            Theme {
                name: BUILT_IN_DARK.to_string(),
                colors: TerminalTheme::dark(),
            },
            Theme {
                name: BUILT_IN_LIGHT.to_string(),
                colors: TerminalTheme::light(),
            },
        ];
        let mut failed = Vec::new();

        let mut files = theme_files(directory);
        files.sort();
        for path in files {
            let name = file_name(&path);
            match read_theme(&path, &name) {
                Ok(theme) => themes.push(theme),
                Err(error) => {
                    log::warn!("theme {}: {error}", path.display());
                    failed.push(name);
                }
            }
        }

        Self { themes, failed }
    }

    /// Names of every palette, in the order they are offered.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.themes.iter().map(|theme| theme.name.as_str())
    }

    /// Palette with this name, or the built-in one of the mode when the name
    /// names nothing, which is what a deleted file leaves behind.
    pub fn colors(&self, name: &str, dark: bool) -> TerminalTheme {
        self.themes
            .iter()
            .find(|theme| theme.name == name)
            .map(|theme| theme.colors.clone())
            .unwrap_or_else(|| built_in(dark))
    }

    /// Names of the files that could not be read.
    pub fn failed(&self) -> &[String] {
        &self.failed
    }
}

/// Palette the application falls back to in one color mode.
pub fn built_in(dark: bool) -> TerminalTheme {
    if dark {
        TerminalTheme::dark()
    } else {
        TerminalTheme::light()
    }
}

/// Name of the dark built-in palette, for a default of the configuration.
pub fn dark_name() -> String {
    BUILT_IN_DARK.to_string()
}

/// Name of the light built-in palette, for a default of the configuration.
pub fn light_name() -> String {
    BUILT_IN_LIGHT.to_string()
}

fn theme_files(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        })
        .collect()
}

fn file_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn read_theme(path: &Path, name: &str) -> Result<Theme> {
    let text = std::fs::read_to_string(path).map_err(|source| AppError::ThemeRead {
        name: name.to_string(),
        source,
    })?;
    let colors = parse(&text, name)?;
    Ok(Theme {
        name: name.to_string(),
        colors,
    })
}

/// Colors of one theme file.
///
/// Everything the file leaves out keeps the value of the built-in palette of
/// the mode the file belongs to, which the background decides: a partial file
/// is a usable file.
pub fn parse(text: &str, name: &str) -> Result<TerminalTheme> {
    let file: ThemeFile = toml::from_str(text).map_err(|source| AppError::ThemeParse {
        name: name.to_string(),
        source: Box::new(source),
    })?;
    let colors = file.colors;

    let background = color(colors.primary.background.as_deref());
    let mut theme = built_in(is_dark(background));

    if let Some(value) = background {
        theme.background = value;
    }
    if let Some(value) = color(colors.primary.foreground.as_deref()) {
        theme.foreground = value;
        theme.cursor = value;
    }
    if let Some(value) = color(colors.primary.bright_foreground.as_deref()) {
        theme.bright_foreground = value;
    }
    if let Some(value) = color(colors.primary.dim_foreground.as_deref()) {
        theme.dim_foreground = value;
    }
    if let Some(value) = color(colors.cursor.cursor.as_deref()) {
        theme.cursor = value;
    }
    if let Some(value) = color(colors.selection.background.as_deref()) {
        theme.selection = value;
    }
    if let Some(value) = color(colors.search.matches.background.as_deref()) {
        theme.search_match = value;
    }
    colors.normal.fill(&mut theme.palette[0..8]);
    colors.bright.fill(&mut theme.palette[8..16]);

    Ok(theme)
}

/// A background this dark asks for the dark palette underneath.
fn is_dark(background: Option<Color32>) -> bool {
    let Some(background) = background else {
        return true;
    };
    let value = u32::from(background.r()) + u32::from(background.g()) + u32::from(background.b());
    value < 3 * 128
}

/// `#rrggbb`, `0xrrggbb` and bare `rrggbb`, the three forms the published
/// themes use.
fn color(value: Option<&str>) -> Option<Color32> {
    let value = value?.trim();
    let digits = value
        .strip_prefix('#')
        .or_else(|| value.strip_prefix("0x"))
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    if digits.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&digits[0..2], 16).ok()?;
    let green = u8::from_str_radix(&digits[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&digits[4..6], 16).ok()?;
    Some(Color32::from_rgb(red, green, blue))
}

#[derive(Debug, Default, Deserialize)]
struct ThemeFile {
    #[serde(default)]
    colors: Colors,
}

#[derive(Debug, Default, Deserialize)]
struct Colors {
    #[serde(default)]
    primary: Primary,
    #[serde(default)]
    cursor: Cursor,
    #[serde(default)]
    selection: Selection,
    #[serde(default)]
    search: Search,
    #[serde(default)]
    normal: Group,
    #[serde(default)]
    bright: Group,
}

#[derive(Debug, Default, Deserialize)]
struct Primary {
    background: Option<String>,
    foreground: Option<String>,
    bright_foreground: Option<String>,
    dim_foreground: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Cursor {
    cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Selection {
    background: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Search {
    #[serde(default)]
    matches: Matches,
}

#[derive(Debug, Default, Deserialize)]
struct Matches {
    background: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Group {
    black: Option<String>,
    red: Option<String>,
    green: Option<String>,
    yellow: Option<String>,
    blue: Option<String>,
    magenta: Option<String>,
    cyan: Option<String>,
    white: Option<String>,
}

impl Group {
    /// Writes the eight colors of this group over the entries it names.
    fn fill(&self, entries: &mut [Color32]) {
        let group = [
            self.black.as_deref(),
            self.red.as_deref(),
            self.green.as_deref(),
            self.yellow.as_deref(),
            self.blue.as_deref(),
            self.magenta.as_deref(),
            self.cyan.as_deref(),
            self.white.as_deref(),
        ];
        for (entry, value) in entries.iter_mut().zip(group) {
            if let Some(value) = color(value) {
                *entry = value;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPLETE: &str = r##"
[colors.primary]
background = "#1d2021"
foreground = "#d4be98"

[colors.cursor]
text = "#1d2021"
cursor = "#d4be98"

[colors.search.matches]
background = "#7c6f64"

[colors.selection]
text = "#1d2021"
background = "#d4be98"

[colors.normal]
black = "#1d2021"
red = "#ea6962"
green = "#a9b665"
yellow = "#d8a657"
blue = "#7daea3"
magenta = "#d3869b"
cyan = "#89b482"
white = "#d4be98"

[colors.bright]
black = "#eddeb5"
red = "#ea6962"
green = "#a9b665"
yellow = "#d8a657"
blue = "#7daea3"
magenta = "#d3869b"
cyan = "#89b482"
white = "#d4be98"
"##;

    fn directory(case: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("zyterm-themes-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn a_complete_file_fills_every_color() {
        let theme = parse(COMPLETE, "gruvbox").expect("the file parses");

        assert_eq!(theme.background, Color32::from_rgb(0x1d, 0x20, 0x21));
        assert_eq!(theme.foreground, Color32::from_rgb(0xd4, 0xbe, 0x98));
        assert_eq!(theme.cursor, Color32::from_rgb(0xd4, 0xbe, 0x98));
        assert_eq!(theme.selection, Color32::from_rgb(0xd4, 0xbe, 0x98));
        assert_eq!(theme.search_match, Color32::from_rgb(0x7c, 0x6f, 0x64));
        assert_eq!(theme.palette[1], Color32::from_rgb(0xea, 0x69, 0x62));
        assert_eq!(theme.palette[8], Color32::from_rgb(0xed, 0xde, 0xb5));
        assert_eq!(theme.palette[15], Color32::from_rgb(0xd4, 0xbe, 0x98));
    }

    #[test]
    fn a_partial_file_keeps_the_built_in_colors() {
        let theme =
            parse("[colors.normal]\nred = \"0xff0000\"\n", "partial").expect("the file parses");
        let built_in = TerminalTheme::dark();

        assert_eq!(theme.palette[1], Color32::from_rgb(0xff, 0x00, 0x00));
        assert_eq!(theme.background, built_in.background);
        assert_eq!(theme.foreground, built_in.foreground);
        assert_eq!(theme.palette[2], built_in.palette[2]);
    }

    #[test]
    fn a_light_background_takes_the_light_palette_underneath() {
        let theme = parse("[colors.primary]\nbackground = \"#fdf6e3\"\n", "light")
            .expect("the file parses");

        assert_eq!(theme.foreground, TerminalTheme::light().foreground);
    }

    #[test]
    fn the_catalog_offers_the_built_ins_and_the_files() {
        let directory = directory("catalog");
        std::fs::create_dir_all(&directory).expect("the directory is created");
        std::fs::write(directory.join("gruvbox.toml"), COMPLETE).expect("the file is written");
        std::fs::write(directory.join("broken.toml"), "colors = [").expect("the file is written");
        std::fs::write(directory.join("notes.txt"), "not a theme").expect("the file is written");

        let catalog = ThemeCatalog::from_directory(&directory);
        let names: Vec<&str> = catalog.names().collect();

        assert_eq!(names, vec![BUILT_IN_DARK, BUILT_IN_LIGHT, "gruvbox"]);
        assert_eq!(catalog.failed(), ["broken".to_string()]);
        assert_eq!(
            catalog.colors("gruvbox", true).background,
            Color32::from_rgb(0x1d, 0x20, 0x21)
        );
        assert_eq!(
            catalog.colors("gone", false),
            TerminalTheme::light(),
            "a name that resolves to nothing falls back to the built-in"
        );

        let _ = std::fs::remove_dir_all(directory);
    }
}
