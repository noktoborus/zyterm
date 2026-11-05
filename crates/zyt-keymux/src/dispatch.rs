//! Context stack, key sequences and the repeat of a binding.

use crate::key::KeyStroke;
use crate::keymap::{Binding, CommandId, Context, Keymap};
use std::time::{Duration, Instant};

/// How long a repeat lasts after each key of it before the caller says.
pub const REPEAT_TIME: Duration = Duration::from_millis(500);

/// Result of one key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
    /// A command was resolved.
    Command(CommandId),
    /// The press begins a longer sequence or goes on with it.
    Pending,
    /// The press broke a sequence: it is taken, and means nothing.
    Dropped,
    /// The press belongs to no binding; the caller may send it on.
    Unhandled,
}

/// Receives keys and resolves them into commands.
///
/// A sequence waits for its next key without a limit. A key that continues
/// no binding drops the sequence and is dropped with it: it was typed for the
/// sequence, not for whatever stands behind. After a binding marked `repeat`
/// ran, a binding of the same keys before the last, marked `repeat` as well,
/// runs by its last key alone, until a pause longer than the repeat time.
#[derive(Debug, Clone)]
pub struct KeyDispatcher {
    keymap: Keymap,
    contexts: Vec<Context>,
    pending: Vec<KeyStroke>,
    repeat: Option<(Vec<KeyStroke>, Instant)>,
    repeat_time: Duration,
}

impl KeyDispatcher {
    /// Dispatcher for the given key map, in the global context.
    pub fn new(keymap: Keymap) -> Self {
        Self {
            keymap,
            contexts: vec![Context::new(Context::GLOBAL)],
            pending: Vec::new(),
            repeat: None,
            repeat_time: REPEAT_TIME,
        }
    }

    /// Replaces the active contexts, in lookup order, the global one last.
    /// What was half typed is dropped.
    pub fn set_contexts(&mut self, contexts: &[&str]) {
        self.contexts = contexts.iter().map(|name| Context::new(name)).collect();
        let global = Context::new(Context::GLOBAL);
        if !self.contexts.contains(&global) {
            self.contexts.push(global);
        }
        self.reset();
    }

    /// Active contexts in lookup order.
    pub fn contexts(&self) -> &[Context] {
        &self.contexts
    }

    /// Key map in use.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// How long a repeat lasts after each key of it.
    pub fn set_repeat_time(&mut self, time: Duration) {
        self.repeat_time = time;
    }

    /// Keys of a sequence that is not finished yet.
    pub fn pending(&self) -> &[KeyStroke] {
        &self.pending
    }

    /// When the repeat running at `now` ends.
    pub fn repeating(&self, now: Instant) -> Option<Instant> {
        let (_, until) = self.repeat.as_ref()?;
        (now <= *until).then_some(*until)
    }

    /// Drops a half typed sequence and a repeat.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.repeat = None;
    }

    /// What can be pressed next, with the keys still to press: the bindings
    /// a sequence goes on with, or those a repeat runs.
    pub fn next(&self, now: Instant) -> Vec<(&[KeyStroke], &Binding)> {
        let (prefix, only_repeat) = match &self.repeat {
            Some((prefix, until)) if self.pending.is_empty() && now <= *until => (prefix, true),
            _ => (&self.pending, false),
        };
        if prefix.is_empty() {
            return Vec::new();
        }
        self.keymap
            .bound(&self.contexts)
            .filter(|binding| binding.repeat || !only_repeat)
            .filter_map(|binding| Some((binding.after(prefix)?, binding)))
            .filter(|(rest, _)| !only_repeat || rest.len() == 1)
            .collect()
    }

    /// Feeds one key press, pressed at `now`.
    pub fn press(&mut self, stroke: KeyStroke, now: Instant) -> Dispatch {
        if let Some((prefix, until)) = self.repeat.take()
            && now <= until
            && self.pending.is_empty()
        {
            let keys = [prefix.as_slice(), std::slice::from_ref(&stroke)].concat();
            if let Some(binding) = self.keymap.find(&self.contexts, &keys)
                && binding.repeat
            {
                let command = binding.command.clone();
                self.repeat = Some((prefix, now + self.repeat_time));
                return Dispatch::Command(command);
            }
        }
        self.repeat = None;
        self.pending.push(stroke);
        if let Some(binding) = self.keymap.find(&self.contexts, &self.pending) {
            let command = binding.command.clone();
            let mut keys = std::mem::take(&mut self.pending);
            keys.pop();
            if binding.repeat && !keys.is_empty() {
                self.repeat = Some((keys, now + self.repeat_time));
            }
            return Dispatch::Command(command);
        }
        let goes_on = self
            .keymap
            .bound(&self.contexts)
            .any(|binding| binding.after(&self.pending).is_some());
        if goes_on {
            return Dispatch::Pending;
        }
        let broken = self.pending.len() > 1;
        self.pending.clear();
        match broken {
            true => Dispatch::Dropped,
            false => Dispatch::Unhandled,
        }
    }
}
