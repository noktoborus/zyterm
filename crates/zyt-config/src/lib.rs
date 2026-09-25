//! Configuration files in the directories the platform expects.
//!
//! Locations come from the `directories` crate: XDG base directories on Linux
//! and the Known Folder API on Windows. Files are YAML and are written
//! atomically through a temporary file in the same directory.
//!
//! Lock files are the exception and live under the temporary directory of the
//! platform, which `tempfile` resolves: `TMPDIR` on Unix, the Known Folder for
//! temporary files on Windows. A lock says that a copy of the application is
//! running now, so it means nothing once the machine has restarted — and a
//! configuration directory that is synchronised between machines, which is
//! what one on a roaming profile or a synchronised home is, would carry those
//! stale locks to every other machine.

#![deny(missing_docs)]

mod error;

pub use error::{ConfigError, Result};

use directories::ProjectDirs;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

/// Identity of the application, used to build the directory names.
#[derive(Debug, Clone)]
pub struct AppId {
    /// Reverse domain qualifier, for example `org`.
    pub qualifier: String,
    /// Organization name.
    pub organization: String,
    /// Application name.
    pub application: String,
}

/// Access to the configuration, data and lock directories of one application.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    config_dir: PathBuf,
    data_dir: PathBuf,
    lock_dir: PathBuf,
}

impl ConfigStore {
    /// Resolves the platform directories for the given application.
    pub fn new(id: &AppId) -> Result<Self> {
        let dirs = ProjectDirs::from(&id.qualifier, &id.organization, &id.application)
            .ok_or(ConfigError::NoHomeDirectory)?;
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
            lock_dir: tempfile::env::temp_dir().join(&id.application),
        })
    }

    /// Uses the given directories directly, for tests and for overrides.
    pub fn with_paths(config_dir: PathBuf, data_dir: PathBuf, lock_dir: PathBuf) -> Self {
        Self {
            config_dir,
            data_dir,
            lock_dir,
        }
    }

    /// Directory holding the configuration files.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Directory holding application data.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Directory holding the lock files, under the temporary directory of the
    /// platform.
    ///
    /// It is not created here: whoever takes a lock makes it, because a copy
    /// that never locks anything should leave nothing behind.
    pub fn lock_dir(&self) -> &Path {
        &self.lock_dir
    }

    /// Path of one configuration file.
    pub fn path(&self, file_name: &str) -> PathBuf {
        self.config_dir.join(file_name)
    }

    /// True when the configuration file exists.
    pub fn exists(&self, file_name: &str) -> bool {
        self.path(file_name).is_file()
    }

    /// Reads one configuration file. Returns `None` when it does not exist.
    pub fn load<T: DeserializeOwned>(&self, file_name: &str) -> Result<Option<T>> {
        let path = self.path(file_name);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        serde_yaml_ng::from_str(&text)
            .map(Some)
            .map_err(|source| ConfigError::Decode { path, source })
    }

    /// Reads one configuration file, writing `fallback` when it is missing.
    pub fn load_or_create<T>(&self, file_name: &str, fallback: impl FnOnce() -> T) -> Result<T>
    where
        T: DeserializeOwned + Serialize,
    {
        match self.load(file_name)? {
            Some(value) => Ok(value),
            None => {
                let value = fallback();
                self.save(file_name, &value)?;
                Ok(value)
            }
        }
    }

    /// Writes one configuration file atomically, unless it already says this.
    ///
    /// A file name may name a directory of its own, `themes/one.yaml`, and the
    /// directory is created with it.
    ///
    /// A write that would change nothing is not made. A caller says what it
    /// holds whenever it might have changed — a device that connected, a value
    /// that was typed, a window that was drawn — and most of those times it
    /// holds what it held before. Writing it anyway costs a file created, a
    /// rename and a new modification time every one of them, which is what a
    /// program watching the directory sees and what a disk that is asleep wakes
    /// up for; and a window that writes a file every frame writes one to every
    /// frame it draws, which is how a console that would not start used to
    /// spend a session.
    pub fn save<T: Serialize>(&self, file_name: &str, value: &T) -> Result<()> {
        let path = self.path(file_name);
        let text = serde_yaml_ng::to_string(value).map_err(|source| ConfigError::Encode {
            path: path.clone(),
            source,
        })?;
        if std::fs::read_to_string(&path).is_ok_and(|kept| kept == text) {
            return Ok(());
        }

        let directory = path.parent().unwrap_or(&self.config_dir).to_path_buf();
        std::fs::create_dir_all(&directory).map_err(|source| ConfigError::CreateDirectory {
            path: directory,
            source,
        })?;
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, text.as_bytes()).map_err(|source| ConfigError::Write {
            path: temporary.clone(),
            source,
        })?;
        std::fs::rename(&temporary, &path).map_err(|source| ConfigError::Write { path, source })
    }
}
