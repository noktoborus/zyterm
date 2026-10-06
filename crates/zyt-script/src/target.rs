//! What a script carries: a direction and the paths it was given.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Direction of a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Local file goes to the device.
    Send,
    /// File comes from the device.
    Receive,
}

impl Direction {
    /// The word a script writes for this direction.
    pub fn name(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }

    /// The direction a word names, or nothing when it names neither.
    pub fn of_name(name: &str) -> Option<Self> {
        match name {
            "send" => Some(Self::Send),
            "receive" => Some(Self::Receive),
            _ => None,
        }
    }
}

/// What a script needs before it can run.
///
/// The order is how much a kind asks for, so a script that names both a file
/// and a list of them asks for the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    /// The script takes no path.
    #[default]
    None,
    /// The script takes one file.
    File,
    /// The script takes any number of files.
    Files,
    /// The script takes one directory.
    Directory,
    /// The script takes any number of directories.
    Directories,
}

impl TargetKind {
    /// True when the user picks more than one path for this kind.
    pub fn is_multiple(self) -> bool {
        matches!(self, Self::Files | Self::Directories)
    }

    /// True when the paths of this kind are directories.
    pub fn is_directory(self) -> bool {
        matches!(self, Self::Directory | Self::Directories)
    }

    /// The word a script writes for this kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::File => "file",
            Self::Files => "files",
            Self::Directory => "directory",
            Self::Directories => "directories",
        }
    }

    /// The kind a word names, or nothing when it names none of them.
    pub fn of_name(name: &str) -> Option<Self> {
        match name {
            "none" => Some(Self::None),
            "file" => Some(Self::File),
            "files" => Some(Self::Files),
            "directory" => Some(Self::Directory),
            "directories" => Some(Self::Directories),
            _ => None,
        }
    }
}

/// Paths handed to a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// Nothing is passed.
    None,
    /// A file path.
    File(&'a Path),
    /// Any number of file paths.
    Files(&'a [PathBuf]),
    /// A directory path.
    Directory(&'a Path),
    /// Any number of directory paths.
    Directories(&'a [PathBuf]),
}

impl Target<'_> {
    /// Kind of this target.
    pub fn kind(&self) -> TargetKind {
        match self {
            Self::None => TargetKind::None,
            Self::File(_) => TargetKind::File,
            Self::Files(_) => TargetKind::Files,
            Self::Directory(_) => TargetKind::Directory,
            Self::Directories(_) => TargetKind::Directories,
        }
    }

    /// Every path of this target, in the order the user gave them.
    ///
    /// A target naming one path answers with that one, so a caller that walks
    /// the list does not have to ask which kind it was given.
    pub fn all(&self) -> Vec<PathBuf> {
        match self {
            Self::None => Vec::new(),
            Self::File(path) | Self::Directory(path) => vec![path.to_path_buf()],
            Self::Files(paths) | Self::Directories(paths) => paths.to_vec(),
        }
    }

    /// Path of this target, when it names at least one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::File(path) | Self::Directory(path) => Some(path),
            Self::Files(paths) | Self::Directories(paths) => paths.first().map(PathBuf::as_path),
            Self::None => None,
        }
    }

    /// Answers with an error when this target is not what the script asks for.
    ///
    /// A script that wants a directory and is handed a file would act on the
    /// wrong thing, and the moment to say so is before anything runs.
    pub fn check(&self, expected: TargetKind) -> crate::Result<()> {
        if expected == TargetKind::None || expected == self.kind() {
            return Ok(());
        }
        Err(crate::ScriptError::TargetMismatch {
            expected,
            given: self.kind(),
        })
    }
}

/// The same paths, owned, so they travel to the thread of a script.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Targets {
    /// What kind of paths these are.
    pub kind: TargetKind,
    /// The paths themselves, in the order the user gave them.
    pub paths: Vec<PathBuf>,
}

impl Targets {
    /// Nothing picked.
    pub fn none() -> Self {
        Self::default()
    }

    /// The first of them, when there is one.
    pub fn path(&self) -> Option<&Path> {
        self.paths.first().map(PathBuf::as_path)
    }
}

impl From<Target<'_>> for Targets {
    fn from(target: Target<'_>) -> Self {
        Self {
            kind: target.kind(),
            paths: target.all(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_written_and_read_under_the_same_word() {
        for kind in [
            TargetKind::None,
            TargetKind::File,
            TargetKind::Files,
            TargetKind::Directory,
            TargetKind::Directories,
        ] {
            assert_eq!(TargetKind::of_name(kind.name()), Some(kind));
        }
    }

    #[test]
    fn a_direction_is_written_and_read_under_the_same_word() {
        for direction in [Direction::Send, Direction::Receive] {
            assert_eq!(Direction::of_name(direction.name()), Some(direction));
        }
    }

    #[test]
    fn one_path_and_a_list_of_them_are_walked_the_same_way() {
        let one = PathBuf::from("/tmp/a");
        let many = vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];
        assert_eq!(Target::File(&one).all(), vec![one.clone()]);
        assert_eq!(Target::Files(&many).all(), many);
        assert_eq!(Target::None.all(), Vec::<PathBuf>::new());
    }

    #[test]
    fn a_script_that_wants_a_directory_refuses_a_file() {
        let file = PathBuf::from("/tmp/a");
        let error = Target::File(&file)
            .check(TargetKind::Directory)
            .expect_err("it is refused");
        assert!(matches!(
            error,
            crate::ScriptError::TargetMismatch {
                expected: TargetKind::Directory,
                given: TargetKind::File
            }
        ));
    }

    #[test]
    fn a_script_that_wants_no_path_takes_whatever_it_is_given() {
        let file = PathBuf::from("/tmp/a");
        assert!(Target::File(&file).check(TargetKind::None).is_ok());
        assert!(Target::None.check(TargetKind::None).is_ok());
    }
}
