//! Where scripts are looked for, and what was found.
//!
//! Four places: the configuration directory, the data directory of the user,
//! the shared directories of the system, and beside the executable. They are
//! searched in that order and the first one carrying a name wins, so the
//! nearer a copy is to the person the stronger it is: what they wrote beats
//! what they installed for themselves, which beats what was installed for the
//! machine, which beats what came in the archive. The copies that lost are
//! remembered as shadowed, and the page of the settings says so rather than
//! leaving somebody to wonder which of two is running.
//!
//! The directories of the platform are asked for rather than written down
//! here: `XDG_DATA_DIRS` on unix through the `xdg` crate, the known folder of
//! shared application data on Windows. The data and configuration directories
//! of the user belong to whoever owns the application identity, so they are
//! handed in.

use crate::error::{Result, ScriptError};
use crate::manifest::Manifest;
use std::path::{Path, PathBuf};

/// The directory of scripts inside each of the places searched.
pub const SCRIPTS: &str = "scripts";

/// One script that was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The name of its directory, which is what everything but a person goes
    /// by.
    pub id: String,
    /// The directory of the script itself, which is where its manifest is.
    pub directory: PathBuf,
    /// The Lua it is started from.
    pub path: PathBuf,
    /// The directory of scripts it was found in.
    pub root: PathBuf,
    /// What it says about itself.
    pub manifest: Manifest,
    /// Directories of the same name, in places searched later.
    pub shadowed: Vec<PathBuf>,
}

impl Entry {
    /// The name a person reads.
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// The manifest of this script, as a path.
    pub fn manifest_path(&self) -> PathBuf {
        self.directory.join(crate::manifest::MANIFEST)
    }
}

/// A file that looks like a script and could not be used as one.
#[derive(Debug, Clone)]
pub struct Problem {
    /// The file.
    pub path: PathBuf,
    /// Why it was left out, in the words of the error.
    pub said: String,
}

/// Every script found in the directories searched.
#[derive(Debug, Clone, Default)]
pub struct Library {
    roots: Vec<PathBuf>,
    entries: Vec<Entry>,
    problems: Vec<Problem>,
}

impl Library {
    /// Reads the manifest of every script of those directories, in that order.
    ///
    /// One directory per script and `Manifest.yaml` in it. Nothing of a script
    /// is run here — a manifest is a file, and what is installed is a question
    /// about files. A directory whose manifest cannot be used does not take
    /// the list with it: it becomes a [`Problem`], because a script somebody
    /// is writing is broken most of the time and the others have to go on
    /// working while it is.
    pub fn load(roots: &[PathBuf]) -> Self {
        let mut library = Self {
            roots: roots.to_vec(),
            ..Self::default()
        };

        for root in roots {
            let found = directories_of(root);
            if !found.is_empty() {
                log::info!("{}: {} script directories", root.display(), found.len());
            }

            for directory in found {
                let id = name_of(&directory);
                if let Some(standing) = library.entries.iter_mut().find(|entry| entry.id == id) {
                    log::info!(
                        "the script {id} of {} stands in for the one in {}",
                        standing.root.display(),
                        directory.display()
                    );
                    standing.shadowed.push(directory);
                    continue;
                }

                match Manifest::of_directory(&id, &directory) {
                    Ok(manifest) => {
                        log::info!(
                            "the script {id} is {} of {}",
                            manifest.name,
                            directory.display()
                        );
                        library.entries.push(Entry {
                            id,
                            path: directory.join(&manifest.entry),
                            directory,
                            root: root.clone(),
                            manifest,
                            shadowed: Vec::new(),
                        });
                    }
                    Err(error) => {
                        let said = said_of(&error);
                        log::warn!("{} is no script: {said}", directory.display());
                        library.problems.push(Problem {
                            path: directory,
                            said,
                        });
                    }
                }
            }
        }

        library.entries.sort_by(|one, two| {
            one.manifest
                .order
                .cmp(&two.manifest.order)
                .then_with(|| one.id.cmp(&two.id))
        });
        library
    }

    /// Every script, in the order they are offered.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The directories that were searched, in the order they were.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// The files that look like scripts and could not be used as one.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// The script of that name, or an error naming what was asked for.
    pub fn find(&self, id: &str) -> Result<&Entry> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| ScriptError::NotFound {
                name: id.to_string(),
            })
    }

    /// Every script that offers that direction.
    pub fn offering(&self, direction: crate::target::Direction) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|entry| entry.manifest.offers(direction))
            .collect()
    }
}

/// The text of an error, with the chain of what caused it.
fn said_of(error: &ScriptError) -> String {
    let mut said = error.to_string();
    let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(error);
    while let Some(next) = cause {
        said.push_str(": ");
        said.push_str(&next.to_string());
        cause = next.source();
    }
    said
}

/// Every directory of that directory that carries a manifest, by name.
///
/// A directory without one is not a script: `lib/` beside them is what
/// `require` reads, and a library is not something somebody can start.
fn directories_of(root: &Path) -> Vec<PathBuf> {
    let Ok(listed) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut found: Vec<PathBuf> = listed
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join(crate::manifest::MANIFEST).is_file())
        .collect();
    found.sort_unstable();
    found
}

/// The name of a directory, which is the name of the script in it.
fn name_of(directory: &Path) -> String {
    directory
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// The directory the running program stands in.
pub fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

/// The directories the program is shipped with its scripts in.
///
/// Its own, and — when it is standing under a build directory — the root of
/// the tree it was built from, because a program run with `cargo run` stands
/// in `target/debug` and a test of it deeper still, and the scripts of the
/// product are not copied to either. It costs walking a few parents and it is
/// what makes a checkout run its own scripts.
pub fn shipped_dirs() -> Vec<PathBuf> {
    let Some(beside) = executable_dir() else {
        return Vec::new();
    };

    let mut found = vec![beside.clone()];
    if let Some(root) = built_from(&beside) {
        found.push(root);
    }
    found
}

/// The root of the tree a program standing under a build directory was built
/// from.
fn built_from(beside: &Path) -> Option<PathBuf> {
    let mut at = Some(beside);
    while let Some(here) = at {
        if here.file_name().is_some_and(|name| name == "target") {
            return here.parent().map(Path::to_path_buf);
        }
        at = here.parent();
    }
    None
}

/// The shared directories of the system, in the order the platform prefers.
pub fn system_dirs(app: &str) -> Vec<PathBuf> {
    #[cfg(unix)]
    {
        xdg::BaseDirectories::with_prefix(app).get_data_dirs()
    }
    #[cfg(windows)]
    {
        known_folders::get_known_folder_path(known_folders::KnownFolder::ProgramData)
            .map(|shared| vec![shared.join(app)])
            .unwrap_or_default()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = app;
        Vec::new()
    }
}

/// The data and configuration directories of the user, as the platform has
/// them.
///
/// The application resolves these from its own identity and hands them in;
/// this is for the runner, which has no identity of its own and is told the
/// name of the application instead. On Windows a program that keeps its
/// settings under an organization name will not agree with this, so the runner
/// takes `--path` as well.
pub fn user_dirs(app: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    #[cfg(unix)]
    {
        let base = xdg::BaseDirectories::with_prefix(app);
        (base.get_data_home(), base.get_config_home())
    }
    #[cfg(windows)]
    {
        let data = known_folders::get_known_folder_path(known_folders::KnownFolder::RoamingAppData)
            .map(|roaming| roaming.join(app));
        (data.clone(), data)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = app;
        (None, None)
    }
}

/// The four places a script may stand, in the order they are searched.
///
/// The configuration directory first, because that is where somebody writing a
/// script works and their copy has to be the one that runs; then their own
/// data directory; then what was installed for every user of the machine; and
/// last what the program was shipped with, which an unpacked archive runs with
/// nothing installed at all.
pub fn default_roots(app: &str, data: Option<&Path>, config: Option<&Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Some(config) = config {
        roots.push(config.join(SCRIPTS));
    }
    if let Some(data) = data {
        roots.push(data.join(SCRIPTS));
    }
    for shared in system_dirs(app) {
        roots.push(shared.join(SCRIPTS));
    }
    for beside in shipped_dirs() {
        roots.push(beside.join(SCRIPTS));
    }

    let mut seen: Vec<PathBuf> = Vec::new();
    roots.retain(|root| {
        if seen.contains(root) {
            return false;
        }
        seen.push(root.clone());
        true
    });
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearer_a_copy_is_to_the_person_the_stronger_it_is() {
        let data = PathBuf::from("/home/one/.local/share/zyterm");
        let config = PathBuf::from("/home/one/.config/zyterm");
        let roots = default_roots("zyterm", Some(&data), Some(&config));

        assert_eq!(roots.first(), Some(&config.join(SCRIPTS)));
        assert_eq!(roots.get(1), Some(&data.join(SCRIPTS)));
        assert_eq!(
            roots.last(),
            shipped_dirs().last().map(|at| at.join(SCRIPTS)).as_ref(),
            "what the program was shipped with is the weakest"
        );
        assert!(roots.len() >= 3, "{roots:?}");
    }

    #[test]
    fn a_place_named_twice_is_searched_once() {
        let one = PathBuf::from("/opt/zyterm");
        let roots = default_roots("zyterm", Some(&one), Some(&one));
        let named = roots
            .iter()
            .filter(|root| **root == one.join(SCRIPTS))
            .count();
        assert_eq!(named, 1);
    }

    #[test]
    fn only_the_directories_carrying_a_manifest_are_scripts() {
        let root = std::env::temp_dir().join(format!("zyt-script-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("one")).expect("the tree is made");
        std::fs::create_dir_all(root.join("lib/fish")).expect("the tree is made");
        std::fs::write(root.join("one/Manifest.yaml"), "name: One\n").expect("written");
        std::fs::write(root.join("one/init.lua"), "return {}").expect("written");
        std::fs::write(root.join("lib/fish/wire.lua"), "return {}").expect("written");
        std::fs::write(root.join("notes.md"), "no").expect("written");

        let found = directories_of(&root);
        assert_eq!(found, vec![root.join("one")]);
        assert_eq!(name_of(&root.join("one")), "one");

        std::fs::remove_dir_all(&root).expect("taken away");
    }
}
