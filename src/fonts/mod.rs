//! The fonts of the window, taken from the ones the system has.
//!
//! Nothing is installed and nothing is copied anywhere — and that is meant
//! literally: a font is not read into memory at all, it is mapped from the file
//! it already lies in. The toolkit keeps the bytes of every font it was given
//! for as long as it draws with it, because any character that has not been
//! drawn yet may need a table of that file; handing it a vector means the whole
//! file stands in memory. Handing it a mapping instead means the pages of the
//! file are read as they are touched and no further: a font of twenty megabytes
//! that answers for a script nobody typed costs its table of characters and
//! nothing else.
//!
//! Every family the platform has is described by its font service, so a font
//! is chosen by its name and read from where it lies (`installed`). The
//! families that carry the scripts of the locale are a list of names
//! (`fallback`), looked for the same way.
//!
//! The terminal and the interface are chosen apart, because they are two
//! different jobs: a terminal wants a font whose every glyph is one cell wide,
//! and the interface wants one that reads well at small sizes. So the terminal
//! has a family of its own, `terminal`, and nothing but the terminal is drawn
//! in it — the fields that show a command or a path are the interface, and they
//! follow the interface font like every other part of the window. The interface
//! stands in the `Proportional` and the `Monospace` family of the toolkit,
//! which are the two every widget asks for.
//!
//! The terminal is given a chain of families and the interface one, because
//! the two are asked different questions: the interface is drawn in the font
//! somebody likes to read, and the terminal shows whatever a device sends —
//! letters, a frame of line drawing, a script of another alphabet — which one
//! family rarely carries whole. The chain is the order the families are asked
//! in, and the toolkit's own fonts and the fonts of the locale stand behind all
//! of them.

use egui::epaint::text::{Blob, FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontDefinitions, FontFamily};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

mod fallback;
mod installed;

pub use installed::families;

/// One family the system has, as the settings offer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// Name to choose it by, which is the name the database knows.
    pub name: String,
    /// True when every glyph of it is one cell wide.
    pub monospaced: bool,
}

/// The family the terminal is drawn in, and nothing else in the window.
///
/// It is a family of the application and not one of the toolkit, so a font
/// chosen for the terminal reaches the terminal alone: `Monospace` is what a
/// text field of the settings asks for, and a field showing a command is part
/// of the interface.
pub fn terminal_family() -> FontFamily {
    FontFamily::Name(TERMINAL.into())
}

/// Name of that family, as the definitions bind it.
const TERMINAL: &str = "terminal";

/// Puts the chosen families in front of the ones the toolkit draws with.
///
/// The terminal is given a chain and not one family: the families stand in the
/// order they were collected, the first one is the font of the grid and every
/// one behind it answers for what the ones in front do not carry. The families
/// the toolkit ships with stay behind all of them, so a glyph nothing chosen
/// carries is still drawn, and the chain the locale asks for is appended last
/// for the same reason. A name that names no installed family is written to the
/// log and otherwise ignored: a font somebody uninstalled must not leave a
/// window unpainted, and it must not take the place of the family behind it in
/// the chain either.
///
/// The terminal family is bound whether or not a font was chosen for it,
/// because a family nothing is bound to is a panic the moment a glyph of it is
/// asked for; with nothing chosen it is the monospaced chain of the toolkit,
/// which is what the terminal was drawn in before anybody chose anything.
///
pub fn apply(context: &egui::Context, terminal: &[String], interface: Option<&str>) {
    let terminal: Vec<&str> = terminal
        .iter()
        .filter_map(|name| named(Some(name)))
        .collect();
    let interface = named(interface);

    let mut definitions = FontDefinitions::default();
    let mut chain = definitions
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let mut links = 0;
    for name in terminal {
        let Some(key) = chosen(&mut definitions, TERMINAL, Some(name)) else {
            continue;
        };
        chain.insert(links, key);
        links += 1;
    }
    definitions.families.insert(terminal_family(), chain);

    if let Some(key) = chosen(&mut definitions, "interface", interface) {
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            definitions
                .families
                .entry(family)
                .or_default()
                .insert(0, key.clone());
        }
    }

    context.set_fonts(definitions);
    add_fallbacks(context);
}

/// Reads the font a setting names into the definitions and answers by the key
/// it was written under, which is nothing when nothing was chosen or nothing of
/// that name is installed.
fn chosen(definitions: &mut FontDefinitions, job: &str, name: Option<&str>) -> Option<String> {
    let name = name?;
    let Some(data) = read(name) else {
        log::warn!("no installed family is named {name}");
        return None;
    };
    let key = format!("{job}:{name}");
    definitions.font_data.insert(key.clone(), Arc::new(data));
    Some(key)
}

/// Appends the fonts of the locale behind everything else.
///
/// The built-in fonts of the toolkit carry the Latin and the Cyrillic alphabets
/// and little else, so a machine whose locale asks for another script needs the
/// fonts of that script from the system, behind the chosen ones: a glyph is
/// looked for here only when nothing in front of it has one. The files are
/// mapped, not read, so a script nobody types costs nothing.
fn add_fallbacks(context: &egui::Context) {
    let locale = sys_locale::get_locale().unwrap_or_default();
    let found: Vec<(&str, FontData)> = fallback::families(&locale)
        .into_iter()
        .filter_map(|name| Some((name, read(name)?)))
        .collect();
    log::info!(
        "locale {locale:?}: {} fonts stand behind the chosen ones",
        found.len()
    );

    for (name, data) in found {
        context.add_font(FontInsert {
            name: format!("fallback:{name}"),
            data,
            families: [
                FontFamily::Proportional,
                FontFamily::Monospace,
                terminal_family(),
            ]
            .into_iter()
            .map(|family| InsertFontFamily {
                family,
                priority: FontPriority::Lowest,
            })
            .collect(),
        });
    }
}

/// The family a setting names, which is nothing when it names nothing.
fn named(chosen: Option<&str>) -> Option<&str> {
    chosen.filter(|name| !name.is_empty())
}

/// The font of one family, mapped from the file it lies in, when the system has
/// that family (`installed::find`).
fn read(name: &str) -> Option<FontData> {
    let face = installed::find(name)?;
    Some(FontData::from_blob(mapped(&face.path)?, face.index))
}

/// The bytes of one font file, mapped, shared with the toolkit.
///
/// A mapping is what a file already is: the pages come in as they are read and
/// the kernel takes them back under pressure. The toolkit holds the mapping for
/// as long as it draws with the font, and lets it go when the settings name
/// another. While it is held, the same file is not mapped again: the settings
/// may hand the same font over again, and a mapping per handing over would be a
/// mapping per change of a setting.
fn mapped(path: &Path) -> Option<Blob> {
    static MAPPED: OnceLock<Mutex<BTreeMap<PathBuf, Weak<memmap2::Mmap>>>> = OnceLock::new();
    let maps = MAPPED.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut maps = maps.lock().unwrap_or_else(|error| error.into_inner());

    maps.retain(|_, map| map.strong_count() > 0);
    if let Some(map) = maps.get(path).and_then(Weak::upgrade) {
        return Some(map);
    }

    let file = std::fs::File::open(path)
        .inspect_err(|error| {
            log::warn!("the font file {} cannot be opened: {error}", path.display())
        })
        .ok()?;
    let map = unsafe { memmap2::Mmap::map(&file) }
        .inspect_err(|error| {
            log::warn!("the font file {} cannot be mapped: {error}", path.display())
        })
        .ok()?;
    let map = Arc::new(map);
    maps.insert(path.to_path_buf(), Arc::downgrade(&map));
    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The terminal has a family of its own and it is bound to fonts, whether
    /// or not a font was ever chosen for it: a family nothing is bound to is a
    /// panic the first time the terminal measures a cell.
    #[test]
    fn the_terminal_family_is_bound_with_nothing_chosen() {
        let context = egui::Context::default();
        apply(&context, &[], None);
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();

        assert_ne!(terminal_family(), FontFamily::Monospace);
        context.fonts_mut(|fonts| {
            assert!(
                !fonts.characters(&terminal_family()).is_empty(),
                "the terminal family carries glyphs"
            );
        });
    }

    /// The chain stands in the order it was collected, and a name nothing is
    /// installed under holds no place in it: the family behind it moves up
    /// rather than standing behind a font the machine has not got.
    #[test]
    fn the_chain_of_the_terminal_stands_in_its_order() {
        let Some(family) = families().first().cloned() else {
            return;
        };
        let context = egui::Context::default();

        apply(
            &context,
            &[
                "a family with this name is nowhere".to_owned(),
                family.name.clone(),
            ],
            None,
        );
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();

        context.fonts(|fonts| {
            let chain = fonts
                .definitions()
                .families
                .get(&terminal_family())
                .expect("the terminal family is bound");
            assert_eq!(
                chain.first().map(String::as_str),
                Some(format!("{TERMINAL}:{}", family.name).as_str()),
                "the family that is installed leads the chain"
            );
        });
    }

    /// The list is what the settings offer, so it names each family once and
    /// stands in an order that does not move between two openings.
    #[test]
    fn the_families_are_named_once_and_in_order() {
        let families = families();

        let names: Vec<&str> = families.iter().map(|one| one.name.as_str()).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "a family is offered once");

        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "the list keeps one order");
        assert!(
            names.iter().all(|name| !name.trim().is_empty()),
            "every family is named"
        );
    }

    /// A family the machine has is read back as the bytes of a font file, and
    /// one it has not is nothing — which is what a setting naming a font
    /// somebody uninstalled must come to.
    #[test]
    fn a_family_is_read_from_the_machine_or_from_nowhere() {
        assert!(read("a family with this name is nowhere").is_none());

        if let Some(family) = families().first() {
            let data = read(&family.name).expect("the family is installed");
            assert!(
                data.bytes().len() > 4,
                "the file of {} holds a font",
                family.name
            );
        }
    }

    /// A file is mapped once while it is held: the same bytes come back, at the
    /// same address, so a setting changed a hundred times maps nothing a
    /// hundred times.
    #[test]
    fn a_font_file_is_mapped_once_while_it_is_held() {
        let Some(face) = families()
            .first()
            .and_then(|family| installed::find(&family.name))
        else {
            return;
        };
        let path = face.path;

        let first = mapped(&path).expect("the file maps");
        let again = mapped(&path).expect("the file maps");

        assert_eq!(
            (*first).as_ref().as_ptr(),
            (*again).as_ref().as_ptr(),
            "the same mapping comes back"
        );
        assert!(mapped(Path::new("/no/such/font.ttf")).is_none());
    }
}
