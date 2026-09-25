//! Command registry and the search behind the command palette.

use crate::key::Chord;
use crate::keymap::{CommandId, Context, Keymap};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// One command the application can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// Identifier used by bindings.
    pub id: CommandId,
    /// Context the command belongs to.
    pub context: Context,
    /// Text shown to the user, already translated by the caller.
    pub title: String,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Command that matched.
    pub command: Command,
    /// Key sequence bound to the command, when there is one.
    pub keys: Option<Chord>,
    /// Match score, higher is better.
    pub score: u32,
}

/// All commands, searchable with a fuzzy pattern.
pub struct CommandRegistry {
    commands: Vec<Command>,
    matcher: Matcher,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandRegistry")
            .field("commands", &self.commands)
            .finish()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Adds one command, replacing an entry with the same identifier.
    pub fn add(&mut self, command: Command) {
        self.commands.retain(|entry| entry.id != command.id);
        self.commands.push(command);
    }

    /// All registered commands.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// Command with the given identifier.
    pub fn get(&self, id: &CommandId) -> Option<&Command> {
        self.commands.iter().find(|command| &command.id == id)
    }

    /// Searches commands of the given contexts.
    ///
    /// An empty query returns every reachable command in registration order.
    /// A non empty query is matched against the title and the identifier.
    pub fn search(&mut self, query: &str, contexts: &[Context], keymap: &Keymap) -> Vec<Hit> {
        let reachable: Vec<Command> = self
            .commands
            .iter()
            .filter(|command| {
                command.context.name() == Context::GLOBAL || contexts.contains(&command.context)
            })
            .cloned()
            .collect();

        let mut hits: Vec<Hit> = if query.trim().is_empty() {
            reachable
                .into_iter()
                .map(|command| Hit {
                    keys: keymap.keys_for(contexts, &command.id).cloned(),
                    command,
                    score: 0,
                })
                .collect()
        } else {
            let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
            let mut buffer = Vec::new();
            reachable
                .into_iter()
                .filter_map(|command| {
                    let haystack = format!("{} {}", command.title, command.id);
                    let score =
                        pattern.score(Utf32Str::new(&haystack, &mut buffer), &mut self.matcher)?;
                    Some(Hit {
                        keys: keymap.keys_for(contexts, &command.id).cloned(),
                        command,
                        score,
                    })
                })
                .collect()
        };

        if !query.trim().is_empty() {
            hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        }
        hits
    }
}
