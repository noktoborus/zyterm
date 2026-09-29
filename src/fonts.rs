//! The fonts of the window, taken from the ones the system has.
//!
//! Nothing is installed and nothing is copied anywhere — and that is meant
//! literally: a font is not read into memory at all, it is mapped from the file
//! it already lies in. The toolkit keeps the bytes of every font it was given
//! for as long as it draws with it, because any character that has not been
//! drawn yet may need a table of that file; handing it a vector means the file
//! is in memory twice over, once in the definitions and once in the blob the
//! toolkit clones out of them. Handing it a mapping instead means the pages of
//! the file are read as they are touched and no further: a font of twenty
//! megabytes that answers for a script nobody typed costs its table of
//! characters and nothing else.
//!
//! Every family the platform has is already described in its font files and in
//! the configuration of its font service, so a font is chosen by its name and
//! read from where it lies: `fontdb` reads those files and the directories
//! `fontconfig` names on Unix, and `system-fonts` says which families carry which
//! script, which is what the fallback chain of the locale is made of.
//!
//! Whether every glyph of a family is one cell wide is read out of the metadata
//! of its faces and not by loading them: a face says so itself, and asking the
//! file instead would mean reading every font of the machine to fill one list.
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

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontDefinitions, FontFamily};
use fontdb::{Database, FaceInfo, Language, Query, Source, Weight};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// One family the system has, as the settings offer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// Name to choose it by, which is the name the database knows.
    pub name: String,
    /// True when every glyph of it is one cell wide.
    pub monospaced: bool,
}

/// Every family the system has, named once, in the order a list shows them.
///
/// Reading the database means reading the font directories of the platform, so
/// this is asked once and kept; the settings do not ask it per frame. A family
/// counts as monospaced when a face of it says it is: the faces of such a family
/// carry the flag together, and a family that carries it nowhere is not one.
pub fn families() -> Vec<Family> {
    let mut monospaced_by_name: BTreeMap<String, bool> = BTreeMap::new();
    for face in database().faces() {
        let Some(name) = family_name(face) else {
            continue;
        };
        *monospaced_by_name.entry(name).or_default() |= face.monospaced;
    }

    monospaced_by_name
        .into_iter()
        .map(|(name, monospaced)| Family { name, monospaced })
        .collect()
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
/// The database is read only when a family was chosen. Reading it means reading
/// every font directory of the platform, which is the longest thing a window
/// with the fonts of the toolkit would wait for before it shows.
pub fn apply(context: &egui::Context, terminal: &[String], interface: Option<&str>) {
    let terminal: Vec<&str> = terminal
        .iter()
        .filter_map(|name| named(Some(name)))
        .collect();
    let interface = named(interface);
    let installed = (!terminal.is_empty() || interface.is_some()).then(database);

    let mut definitions = FontDefinitions::default();
    let mut chain = definitions
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let mut links = 0;
    for name in terminal {
        let Some(key) = chosen(&mut definitions, installed.as_ref(), TERMINAL, Some(name)) else {
            continue;
        };
        chain.insert(links, key);
        links += 1;
    }
    definitions.families.insert(terminal_family(), chain);

    if let Some(key) = chosen(&mut definitions, installed.as_ref(), "interface", interface) {
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
fn chosen(
    definitions: &mut FontDefinitions,
    installed: Option<&Database>,
    job: &str,
    name: Option<&str>,
) -> Option<String> {
    let (name, installed) = (name?, installed?);
    let Some(data) = read(installed, name) else {
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
/// looked for here only when nothing in front of it has one. `system-fonts` says
/// which families those are — it knows the names the scripts are shipped under —
/// and the files are mapped, not read, because a script nobody types costs
/// nothing that way.
fn add_fallbacks(context: &egui::Context) {
    let (locale, region, found) =
        system_fonts::find_for_system_locale(system_fonts::FontStyle::Sans);
    log::info!(
        "locale {locale:?} is {region:?}, {} fonts stand behind the chosen ones",
        found.len()
    );

    for font in found {
        let Some(data) = font_data(&font.source) else {
            continue;
        };
        context.add_font(FontInsert {
            name: font.key,
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

/// The bytes of a font the system named, mapped from their file.
///
/// A font the database holds in memory rather than in a file is the exception —
/// nothing here puts one there — and it is copied, because there is no file to
/// map.
fn font_data(source: &system_fonts::FoundFontSource) -> Option<FontData> {
    match source {
        system_fonts::FoundFontSource::Path(path) => mapped(path).map(FontData::from_static),
        system_fonts::FoundFontSource::Bytes(bytes) => Some(FontData::from_owned(bytes.to_vec())),
    }
}

/// The families the platform has, read from its font files.
fn database() -> Database {
    let mut database = Database::new();
    database.load_system_fonts();
    database
}

/// What a face calls its family, in English when it says so.
///
/// A face names its family once per language it was translated into, and the
/// name a list offers has to be the same one a setting is written with — so the
/// English name is taken wherever there is one, and the first name otherwise.
fn family_name(face: &FaceInfo) -> Option<String> {
    face.families
        .iter()
        .find(|(_, language)| *language == Language::English_UnitedStates)
        .or_else(|| face.families.first())
        .map(|(name, _)| name.clone())
}

/// The family a setting names, which is nothing when it names nothing.
fn named(chosen: Option<&str>) -> Option<&str> {
    chosen.filter(|name| !name.is_empty())
}

/// The font of one family, mapped from the file it lies in, when the system has
/// that family.
///
/// A family is several files, and the one a window wants is the one somebody
/// means by the name of the family: upright, of ordinary weight and ordinary
/// width. A name nobody has installed is nothing at all — the database is asked
/// which faces carry that name and not which face it would put in its place —
/// so a font somebody uninstalled leaves a line in the log instead of a window
/// in a face nobody chose.
fn read(installed: &Database, name: &str) -> Option<FontData> {
    let id = installed.query(&Query {
        families: &[fontdb::Family::Name(name)],
        weight: Weight::NORMAL,
        ..Query::default()
    })?;
    let (source, index) = installed.face_source(id)?;
    let data = match &source {
        Source::File(path) => FontData::from_static(mapped(path)?),
        Source::Binary(bytes) | Source::SharedFile(_, bytes) => {
            FontData::from_owned(bytes.as_ref().as_ref().to_vec())
        }
    };
    Some(FontData { index, ..data })
}

/// The bytes of one font file, mapped once and kept for as long as the program
/// runs.
///
/// The toolkit wants bytes that outlive it, and a mapping is what a file already
/// is: the pages come in as they are read and the kernel takes them back under
/// pressure. Every file is mapped once — the settings may hand the same font
/// over again, and a mapping per handing over would be a mapping per change of a
/// setting.
fn mapped(path: &Path) -> Option<&'static [u8]> {
    static MAPPED: OnceLock<Mutex<BTreeMap<PathBuf, &'static [u8]>>> = OnceLock::new();
    let maps = MAPPED.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut maps = maps.lock().unwrap_or_else(|error| error.into_inner());

    if let Some(bytes) = maps.get(path) {
        return Some(bytes);
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
    let bytes: &'static [u8] = &*Box::leak(Box::new(map));
    maps.insert(path.to_path_buf(), bytes);
    Some(bytes)
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
                !fonts.fonts.font(&terminal_family()).characters().is_empty(),
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
        let installed = database();

        assert!(read(&installed, "a family with this name is nowhere").is_none());

        if let Some(family) = families().first() {
            let data = read(&installed, &family.name).expect("the family is installed");
            assert!(
                data.font.len() > 4,
                "the file of {} holds a font",
                family.name
            );
        }
    }

    /// A file is mapped once: the same bytes come back, at the same address, so
    /// a setting changed a hundred times maps nothing a hundred times.
    #[test]
    fn a_font_file_is_mapped_once_and_kept() {
        let installed = database();
        let Some(family) = families().first().cloned() else {
            return;
        };
        let id = installed
            .query(&Query {
                families: &[fontdb::Family::Name(&family.name)],
                ..Query::default()
            })
            .expect("the family is installed");
        let Some((Source::File(path), _)) = installed.face_source(id) else {
            return;
        };

        let first = mapped(&path).expect("the file maps");
        let again = mapped(&path).expect("the file maps");

        assert_eq!(
            first.as_ptr(),
            again.as_ptr(),
            "the same mapping comes back"
        );
        assert!(mapped(Path::new("/no/such/font.ttf")).is_none());
    }
}
