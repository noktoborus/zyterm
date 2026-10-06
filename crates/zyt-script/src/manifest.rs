//! What a script says about itself, in a file of its own.
//!
//! One directory per script, and `Manifest.yaml` in it. The manifest is read
//! and nothing else: starting the program finds what is installed without
//! running a line of anybody's Lua, which is the difference between a list of
//! what can be done and a list of files that have already done something.
//!
//! It carries the name a person reads — `ZModem`, `Shell Transfer` — while the
//! directory carries the name everything else goes by.

use crate::error::{Result, ScriptError};
use crate::target::{Direction, TargetKind};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The file one script says what it is in.
pub const MANIFEST: &str = "Manifest.yaml";

/// The Lua a script is started from, when the manifest names no other.
pub const DEFAULT_ENTRY: &str = "init.lua";

/// Where a script stands in a list when it says nothing about it.
const DEFAULT_ORDER: i64 = 100;

/// What one direction of a script needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectionSpec {
    /// What the user has to pick before it can run.
    #[serde(default)]
    pub target: TargetKind,
    /// The key sent once the run is over, in its text form.
    #[serde(default)]
    pub finish: String,
}

/// What a script says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The name a person reads.
    pub name: String,
    /// Where it stands among the others; the smaller comes first.
    #[serde(default = "default_order")]
    pub order: i64,
    /// True when it takes the line of the session for as long as it runs.
    #[serde(default = "yes")]
    pub hold_line: bool,
    /// The Lua it is started from, inside its own directory.
    #[serde(default = "default_entry")]
    pub entry: String,
    /// Values of the source it asks for by name.
    #[serde(default)]
    pub variables: Vec<String>,
    /// What it does to send, when it sends at all.
    #[serde(default)]
    pub send: Option<DirectionSpec>,
    /// What it does to receive, when it receives at all.
    #[serde(default)]
    pub receive: Option<DirectionSpec>,
}

/// The order of a script that does not say.
fn default_order() -> i64 {
    DEFAULT_ORDER
}

/// True, which is what a script that does not say means.
///
/// Every script shipped with the program is on the line, so a manifest that
/// leaves the field out is read as the kind it most likely holds.
fn yes() -> bool {
    true
}

/// The name of the file a script is started from, when it names no other.
fn default_entry() -> String {
    DEFAULT_ENTRY.to_string()
}

impl Manifest {
    /// Reads the manifest of the script in that directory.
    ///
    /// `id` is the name of the directory, which is what everything but a
    /// person goes by. The manifest has to offer a direction and the Lua it
    /// names has to be there: a script that cannot be started is better left
    /// out of the list with a word about why than offered and refused.
    pub fn of_directory(id: &str, directory: &Path) -> Result<Self> {
        let path = directory.join(MANIFEST);
        let text = std::fs::read_to_string(&path).map_err(|source| ScriptError::Read {
            path: path.clone(),
            source,
        })?;

        let manifest: Self =
            serde_yaml_ng::from_str(&text).map_err(|error| ScriptError::Manifest {
                name: id.to_string(),
                what: error.to_string(),
            })?;

        let wrong = |what: &str| ScriptError::Manifest {
            name: id.to_string(),
            what: what.to_string(),
        };

        if manifest.name.trim().is_empty() {
            return Err(wrong("it has no name"));
        }
        if !manifest.offers(Direction::Send) && !manifest.offers(Direction::Receive) {
            return Err(wrong("it offers neither direction"));
        }
        if !directory.join(&manifest.entry).is_file() {
            return Err(wrong(&format!(
                "it names {}, which is not there",
                manifest.entry
            )));
        }
        for direction in [Direction::Send, Direction::Receive] {
            crate::finish::finish_bytes(manifest.finish(direction))?;
        }

        Ok(manifest)
    }

    /// What that direction needs, when the script offers it.
    pub fn direction(&self, direction: Direction) -> Option<&DirectionSpec> {
        match direction {
            Direction::Send => self.send.as_ref(),
            Direction::Receive => self.receive.as_ref(),
        }
    }

    /// True when the script offers that direction at all.
    pub fn offers(&self, direction: Direction) -> bool {
        self.direction(direction).is_some()
    }

    /// What the user has to pick for that direction.
    pub fn target_kind(&self, direction: Direction) -> TargetKind {
        self.direction(direction)
            .map(|spec| spec.target)
            .unwrap_or(TargetKind::None)
    }

    /// The key that direction sends when it is over.
    pub fn finish(&self, direction: Direction) -> &str {
        self.direction(direction)
            .map(|spec| spec.finish.as_str())
            .unwrap_or(crate::finish::FINISH_NONE)
    }
}
