//! Bindings grouped by context.

use crate::error::{KeymapError, Result};
use crate::key::{Chord, KeyStroke};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Name of an input context, for example `terminal` or `settings`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Context(pub String);

impl Context {
    /// Context that is always active.
    pub const GLOBAL: &'static str = "global";

    /// Context from a string slice.
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Name of the context.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Context {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Identifier of a command, for example `terminal.copy`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CommandId(pub String);

impl CommandId {
    /// Identifier from a string slice.
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }

    /// Text of the identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CommandId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One key sequence bound to one command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// Key sequence.
    pub keys: Chord,
    /// Command triggered by the sequence.
    pub command: CommandId,
}

/// All bindings of all contexts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    /// Bindings per context name.
    pub contexts: BTreeMap<Context, Vec<Binding>>,
}

impl Keymap {
    /// Empty key map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one binding, reporting a conflict inside the same context.
    pub fn bind(&mut self, context: &str, keys: &str, command: &str) -> Result<()> {
        let chord = Chord::parse(keys)?;
        let entry = self.contexts.entry(Context::new(context)).or_default();
        if let Some(existing) = entry.iter().find(|binding| binding.keys == chord) {
            return Err(KeymapError::Conflict {
                context: context.to_string(),
                keys: chord.to_text(),
                existing: existing.command.to_string(),
                incoming: command.to_string(),
            });
        }
        entry.push(Binding {
            keys: chord,
            command: CommandId::new(command),
        });
        Ok(())
    }

    /// Replaces every binding of one context.
    pub fn set_context(&mut self, context: &str, bindings: Vec<Binding>) {
        self.contexts.insert(Context::new(context), bindings);
    }

    /// Bindings of one context.
    pub fn bindings(&self, context: &Context) -> &[Binding] {
        self.contexts
            .get(context)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Key sequence bound to a command, searched over the given contexts.
    pub fn keys_for(&self, contexts: &[Context], command: &CommandId) -> Option<&Chord> {
        contexts.iter().find_map(|context| {
            self.bindings(context)
                .iter()
                .find(|binding| &binding.command == command)
                .map(|binding| &binding.keys)
        })
    }

    /// Reports every key sequence bound twice inside one context.
    pub fn conflicts(&self) -> Vec<KeymapError> {
        let mut problems = Vec::new();
        for (context, bindings) in &self.contexts {
            for (index, binding) in bindings.iter().enumerate() {
                if let Some(earlier) = bindings[..index]
                    .iter()
                    .find(|other| other.keys == binding.keys)
                {
                    problems.push(KeymapError::Conflict {
                        context: context.to_string(),
                        keys: binding.keys.to_text(),
                        existing: earlier.command.to_string(),
                        incoming: binding.command.to_string(),
                    });
                }
            }
        }
        problems
    }

    /// Bindings of `other` are added, replacing bindings with the same keys.
    pub fn merge(&mut self, other: Keymap) {
        for (context, bindings) in other.contexts {
            let entry = self.contexts.entry(context).or_default();
            for binding in bindings {
                entry.retain(|existing| existing.keys != binding.keys);
                entry.push(binding);
            }
        }
    }

    /// Parses a key map from YAML.
    pub fn from_yaml(text: &str) -> Result<Self> {
        serde_yaml_ng::from_str(text).map_err(|source| KeymapError::Decode { source })
    }

    /// Writes the key map as YAML.
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml_ng::to_string(self).map_err(|source| KeymapError::Encode { source })
    }

    /// Commands reachable in the given contexts, with their key sequence.
    pub fn commands_in(&self, contexts: &[Context]) -> Vec<(CommandId, Chord)> {
        let mut found = Vec::new();
        for context in contexts {
            for binding in self.bindings(context) {
                found.push((binding.command.clone(), binding.keys.clone()));
            }
        }
        found
    }

    /// True when some binding of the given contexts continues with `prefix`.
    pub fn has_prefix(&self, contexts: &[Context], prefix: &[KeyStroke]) -> bool {
        contexts.iter().any(|context| {
            self.bindings(context).iter().any(|binding| {
                binding.keys.strokes.len() > prefix.len() && binding.keys.starts_with(prefix)
            })
        })
    }

    /// Command bound exactly to `strokes` in the given contexts.
    pub fn command_for(&self, contexts: &[Context], strokes: &[KeyStroke]) -> Option<CommandId> {
        contexts.iter().find_map(|context| {
            self.bindings(context)
                .iter()
                .find(|binding| binding.keys.strokes == strokes)
                .map(|binding| binding.command.clone())
        })
    }
}
