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
    /// After it ran, its last key runs it again without the keys before it,
    /// for as long as the dispatcher repeats (`bind -r` of tmux).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub repeat: bool,
}

impl Binding {
    /// Binding parsed from the text form of its keys.
    pub fn parse(keys: &str, command: &str, repeat: bool) -> Result<Self> {
        Ok(Self {
            keys: Chord::parse(keys)?,
            command: CommandId::new(command),
            repeat,
        })
    }

    /// The keys of it that follow `prefix`, when it begins with them and goes
    /// further.
    pub fn after(&self, prefix: &[KeyStroke]) -> Option<&[KeyStroke]> {
        let rest = self.keys.strokes.strip_prefix(prefix)?;
        (!rest.is_empty()).then_some(rest)
    }
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
    pub fn bind(&mut self, context: &str, binding: Binding) -> Result<()> {
        let entry = self.contexts.entry(Context::new(context)).or_default();
        if let Some(existing) = entry.iter().find(|other| other.keys == binding.keys) {
            return Err(conflict(context, existing, &binding));
        }
        entry.push(binding);
        Ok(())
    }

    /// Bindings of one context.
    pub fn bindings(&self, context: &Context) -> &[Binding] {
        self.contexts
            .get(context)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Bindings of the given contexts, in their order.
    pub fn bound<'a, 'c>(
        &'a self,
        contexts: &'c [Context],
    ) -> impl Iterator<Item = &'a Binding> + use<'a, 'c> {
        contexts.iter().flat_map(|context| self.bindings(context))
    }

    /// Binding of exactly these keys, the first context that has one.
    pub fn find(&self, contexts: &[Context], strokes: &[KeyStroke]) -> Option<&Binding> {
        self.bound(contexts)
            .find(|binding| binding.keys.strokes == strokes)
    }

    /// Command bound exactly to these keys.
    pub fn command_for(&self, contexts: &[Context], strokes: &[KeyStroke]) -> Option<CommandId> {
        self.find(contexts, strokes)
            .map(|binding| binding.command.clone())
    }

    /// Key sequence bound to a command.
    pub fn keys_for(&self, contexts: &[Context], command: &CommandId) -> Option<&Chord> {
        self.bound(contexts)
            .find(|binding| &binding.command == command)
            .map(|binding| &binding.keys)
    }

    /// Every key sequence bound twice inside one context.
    pub fn conflicts(&self) -> Vec<KeymapError> {
        let mut problems = Vec::new();
        for (context, bindings) in &self.contexts {
            for (index, binding) in bindings.iter().enumerate() {
                if let Some(earlier) = bindings[..index]
                    .iter()
                    .find(|other| other.keys == binding.keys)
                {
                    problems.push(conflict(context.name(), earlier, binding));
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
}

fn conflict(context: &str, existing: &Binding, incoming: &Binding) -> KeymapError {
    KeymapError::Conflict {
        context: context.to_string(),
        keys: incoming.keys.to_text(),
        existing: existing.command.to_string(),
        incoming: incoming.command.to_string(),
    }
}
