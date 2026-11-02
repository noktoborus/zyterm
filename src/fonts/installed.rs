//! The fonts the system has: its families, and the file a family is drawn from.
//!
//! On Linux fontconfig answers both from its cache, which the desktop keeps up
//! to date, so no font file is read to answer: the window opens the files it
//! draws with and no others. On Windows `fontdb` reads the font directory the
//! first time it is asked and keeps what it read, because Windows has no such
//! cache. Either way a font installed while the program runs is seen after a
//! restart.

use super::Family;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// One face of a font file: where the file lies and which face of it is meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    /// The font file.
    pub path: PathBuf,
    /// The face within the file; a collection file holds several.
    pub index: u32,
}

/// Every family the system has, named once, in the order a list shows them.
///
/// A family counts as monospaced when a face of it says it is: the faces of
/// such a family carry the flag together, and a family that carries it nowhere
/// is not one.
pub fn families() -> Vec<Family> {
    let mut monospaced_by_name: BTreeMap<String, bool> = BTreeMap::new();
    for (name, monospaced) in faces_of_families() {
        *monospaced_by_name.entry(name).or_default() |= monospaced;
    }

    monospaced_by_name
        .into_iter()
        .map(|(name, monospaced)| Family { name, monospaced })
        .collect()
}

/// Every face of the system as the name of its family, and whether the face
/// says each of its glyphs is one cell wide.
///
/// A face of two cell widths counts as one: it is the face of a script whose
/// glyphs a terminal draws two cells wide.
#[cfg(target_os = "linux")]
fn faces_of_families() -> Vec<(String, bool)> {
    use fontconfig::{FC_DUAL, FC_FAMILY, FC_SPACING};

    linux::list(None, &[FC_FAMILY, FC_SPACING], |face| {
        let name = face.get_string(FC_FAMILY).ok()?.to_owned();
        let monospaced = face
            .get_int(FC_SPACING)
            .is_ok_and(|spacing| spacing >= FC_DUAL);
        Some((name, monospaced))
    })
}

/// Every face of the system as the name of its family, and whether the face
/// says each of its glyphs is one cell wide.
#[cfg(target_os = "windows")]
fn faces_of_families() -> Vec<(String, bool)> {
    use fontdb::Language;

    windows::database()
        .faces()
        .filter_map(|face| {
            let name = face
                .families
                .iter()
                .find(|(_, language)| *language == Language::English_UnitedStates)
                .or_else(|| face.families.first())?
                .0
                .clone();
            Some((name, face.monospaced))
        })
        .collect()
}

/// The face a family is drawn in, when the system has that family.
///
/// A family is several faces, and the one a window wants is the one somebody
/// means by the name of the family: upright, of ordinary weight and ordinary
/// width. A name nobody has installed is nothing at all — the faces that carry
/// that name are asked for, not the face the system would put in its place —
/// so a font somebody uninstalled leaves a line in the log instead of a window
/// in a face nobody chose.
#[cfg(target_os = "linux")]
pub fn find(name: &str) -> Option<Face> {
    use fontconfig::{
        FC_FILE, FC_INDEX, FC_SLANT, FC_SLANT_ROMAN, FC_WEIGHT, FC_WEIGHT_REGULAR, FC_WIDTH,
        FC_WIDTH_NORMAL,
    };

    let faces = linux::list(
        Some(name),
        &[FC_FILE, FC_INDEX, FC_SLANT, FC_WEIGHT, FC_WIDTH],
        |face| {
            let face_index = face.face_index().unwrap_or(0) & linux::FACE_OF_INDEX;
            let face_found = Face {
                path: PathBuf::from(face.filename().ok()?),
                index: u32::try_from(face_index).ok()?,
            };
            let distance = (
                face.slant().unwrap_or(FC_SLANT_ROMAN) != FC_SLANT_ROMAN,
                (face.weight().unwrap_or(FC_WEIGHT_REGULAR) - FC_WEIGHT_REGULAR).abs(),
                (face.width().unwrap_or(FC_WIDTH_NORMAL) - FC_WIDTH_NORMAL).abs(),
            );
            Some((distance, face_found))
        },
    );
    faces
        .into_iter()
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, face)| face)
}

/// The face a family is drawn in, when the system has that family.
#[cfg(target_os = "windows")]
pub fn find(name: &str) -> Option<Face> {
    use fontdb::{Query, Source, Weight};

    let database = windows::database();
    let id = database.query(&Query {
        families: &[fontdb::Family::Name(name)],
        weight: Weight::NORMAL,
        ..Query::default()
    })?;
    match database.face_source(id)? {
        (Source::File(path), index) => Some(Face { path, index }),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::OnceLock;
    use fontconfig::{FC_FAMILY, Fontconfig, ObjectSet, Pattern, list_fonts};
    use std::ffi::{CStr, CString};

    /// The bits of a fontconfig index that name the face of the file; the bits
    /// above them name an instance of a variable face, which the toolkit draws
    /// at its default.
    pub const FACE_OF_INDEX: i32 = 0xFFFF;

    /// The faces fontconfig lists, every face of the family named or every face
    /// of the system, each read by `read` from the properties named.
    ///
    /// fontconfig that cannot be started or cannot list is a line in the log
    /// and no faces: the window is drawn in the fonts of the toolkit then.
    pub fn list<T>(
        family: Option<&str>,
        properties: &[&CStr],
        read: impl Fn(&Pattern<'_>) -> Option<T>,
    ) -> Vec<T> {
        let Some(fontconfig) = fontconfig() else {
            return Vec::new();
        };
        let listed = || -> Result<Vec<T>, fontconfig::FontconfigError> {
            let mut pattern = Pattern::new(fontconfig)?;
            if let Some(family) = family {
                pattern.add_string(FC_FAMILY, &CString::new(family)?)?;
            }
            let mut objects = ObjectSet::new(fontconfig)?;
            for property in properties {
                objects.add(property)?;
            }
            let faces = list_fonts(&pattern, Some(&objects))?;
            Ok(faces.iter().filter_map(|face| read(&face)).collect())
        };
        listed().unwrap_or_else(|error| {
            log::warn!("fontconfig cannot list the fonts: {error:?}");
            Vec::new()
        })
    }

    /// fontconfig, started once with its configuration and its cache.
    fn fontconfig() -> Option<&'static Fontconfig> {
        static FONTCONFIG: OnceLock<Option<Fontconfig>> = OnceLock::new();
        FONTCONFIG
            .get_or_init(|| {
                let started = Fontconfig::new();
                if started.is_none() {
                    log::warn!("fontconfig cannot be started, no font of the system is used");
                }
                started
            })
            .as_ref()
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::OnceLock;
    use fontdb::Database;

    /// The fonts of the system, read from the font directory the first time
    /// they are asked for and kept.
    pub fn database() -> &'static Database {
        static INSTALLED: OnceLock<Database> = OnceLock::new();
        INSTALLED.get_or_init(|| {
            let mut database = Database::new();
            database.load_system_fonts();
            database
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name nothing is installed under is no face at all, not the face the
    /// system would draw in its place.
    #[test]
    fn a_family_nothing_is_installed_under_is_no_face() {
        assert_eq!(find("a family with this name is nowhere"), None);
    }

    /// Of the faces of a family the upright one of ordinary weight is taken:
    /// the face somebody means by the name.
    #[test]
    fn the_face_of_a_family_is_the_upright_regular_one() {
        let Some(face) = find("DejaVu Sans") else {
            return;
        };
        assert_eq!(
            face.path.file_name().and_then(|name| name.to_str()),
            Some("DejaVuSans.ttf")
        );
    }

    /// Every family the list offers is one a face is found for, so a family
    /// picked from the list is a family the window draws in.
    #[test]
    fn every_family_offered_is_found() {
        for family in families() {
            assert!(find(&family.name).is_some(), "{} is found", family.name);
        }
    }
}
