//! Context stack and key sequence resolution.

use crate::key::KeyStroke;
use crate::keymap::{CommandId, Context, Keymap};

/// Result of one key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
    /// A command was resolved.
    Command(CommandId),
    /// The press begins a longer sequence; more keys are expected.
    Pending,
    /// The press belongs to no binding of the active contexts.
    Unhandled,
}

/// Receives keys and resolves them into commands.
///
/// The dispatcher owns no user interface state. Which contexts are active is
/// decided by the caller through [`KeyDispatcher::set_contexts`].
#[derive(Debug, Clone)]
pub struct KeyDispatcher {
    keymap: Keymap,
    contexts: Vec<Context>,
    pending: Vec<KeyStroke>,
}

/// Receives the commands a key press resolved to.
pub trait ActionSink {
    /// Called once per resolved command.
    fn run(&mut self, command: &CommandId);
}

impl<F: FnMut(&CommandId)> ActionSink for F {
    fn run(&mut self, command: &CommandId) {
        self(command)
    }
}

impl KeyDispatcher {
    /// Dispatcher for the given key map. The global context is always last.
    pub fn new(keymap: Keymap) -> Self {
        Self {
            keymap,
            contexts: vec![Context::new(Context::GLOBAL)],
            pending: Vec::new(),
        }
    }

    /// Replaces the active contexts. The order is the lookup order; the global
    /// context is appended when it is missing.
    pub fn set_contexts(&mut self, contexts: &[&str]) {
        self.contexts = contexts.iter().map(|name| Context::new(name)).collect();
        let global = Context::new(Context::GLOBAL);
        if !self.contexts.contains(&global) {
            self.contexts.push(global);
        }
        self.pending.clear();
    }

    /// Active contexts in lookup order.
    pub fn contexts(&self) -> &[Context] {
        &self.contexts
    }

    /// Key map in use.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Replaces the key map and drops a half finished sequence.
    pub fn set_keymap(&mut self, keymap: Keymap) {
        self.keymap = keymap;
        self.pending.clear();
    }

    /// Keys of a sequence that is not finished yet.
    pub fn pending(&self) -> &[KeyStroke] {
        &self.pending
    }

    /// Drops a half finished sequence.
    pub fn reset(&mut self) {
        self.pending.clear();
    }

    /// Feeds one key press.
    pub fn press(&mut self, stroke: KeyStroke) -> Dispatch {
        self.pending.push(stroke);
        if let Some(command) = self.keymap.command_for(&self.contexts, &self.pending) {
            self.pending.clear();
            return Dispatch::Command(command);
        }
        if self.keymap.has_prefix(&self.contexts, &self.pending) {
            return Dispatch::Pending;
        }
        self.pending.clear();
        Dispatch::Unhandled
    }

    /// Feeds one key press and hands a resolved command to `sink`.
    pub fn press_into(&mut self, stroke: KeyStroke, sink: &mut dyn ActionSink) -> Dispatch {
        let dispatch = self.press(stroke);
        if let Dispatch::Command(command) = &dispatch {
            sink.run(command);
        }
        dispatch
    }
}
