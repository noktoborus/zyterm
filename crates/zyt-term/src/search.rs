//! Search over the grid and the scrollback.
//!
//! How the query is read is one choice of four, and every one of them is one
//! pattern for the same engine, so the terminal never carries more than one
//! search at a time. A whole word is the exception: the word boundary of a
//! regular expression is either rejected by the engine the backend builds or
//! blind to every alphabet but the latin one, so the cells around a match are
//! looked at instead.

use serde::{Deserialize, Serialize};

/// How the query is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchKind {
    /// The query is the text to find, character for character.
    #[default]
    Literal,
    /// The characters of the query are found in order, but not next to each
    /// other.
    Fuzzy,
    /// The query is found only where a word begins and ends.
    Word,
    /// The query is a pattern, not a text.
    Regex,
}

impl SearchKind {
    /// Every kind, in the order a caller offers them.
    pub const ALL: [Self; 4] = [Self::Literal, Self::Fuzzy, Self::Word, Self::Regex];
}

/// What the pattern of a search is built from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchOptions {
    /// How the query is read.
    pub kind: SearchKind,
    /// An upper case letter of the query finds only an upper case letter.
    pub case_sensitive: bool,
    /// Every match on the screen is marked, not only the current one.
    pub highlight_all: bool,
}

/// Which way a search walks the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// Towards the beginning of the scrollback.
    Up,
    /// Towards the newest line.
    Down,
}

/// Builds the pattern the backend is given.
pub(crate) fn pattern_for(query: &str, options: &SearchOptions) -> String {
    let body = match options.kind {
        SearchKind::Regex => query.to_string(),
        SearchKind::Fuzzy => fuzzy(query),
        SearchKind::Literal | SearchKind::Word => escape(query),
    };
    if options.case_sensitive {
        body
    } else {
        format!("(?i){body}")
    }
}

/// Turns a literal into a pattern that matches exactly it.
pub(crate) fn escape(literal: &str) -> String {
    let mut pattern = String::with_capacity(literal.len());
    for character in literal.chars() {
        if character.is_ascii_punctuation() {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

/// Turns a literal into a pattern that finds its characters in order, with
/// anything at all between them.
fn fuzzy(literal: &str) -> String {
    let mut pattern = String::with_capacity(literal.len() * 4);
    for (index, character) in literal.chars().enumerate() {
        if index > 0 {
            pattern.push_str(".*?");
        }
        pattern.push_str(&escape(&character.to_string()));
    }
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> SearchOptions {
        SearchOptions::default()
    }

    #[test]
    fn a_literal_is_escaped_and_the_case_is_dropped() {
        assert_eq!(pattern_for("a.c", &options()), "(?i)a\\.c");
    }

    #[test]
    fn the_case_is_kept_when_it_is_asked_for() {
        let options = SearchOptions {
            case_sensitive: true,
            ..options()
        };
        assert_eq!(pattern_for("Abc", &options), "Abc");
    }

    #[test]
    fn a_whole_word_does_not_reach_the_pattern() {
        let options = SearchOptions {
            kind: SearchKind::Word,
            case_sensitive: true,
            ..options()
        };
        assert_eq!(pattern_for("cat", &options), "cat");
    }

    #[test]
    fn a_regular_expression_is_taken_as_it_stands() {
        let options = SearchOptions {
            kind: SearchKind::Regex,
            case_sensitive: true,
            ..options()
        };
        assert_eq!(pattern_for("a.c", &options), "a.c");
    }

    #[test]
    fn a_fuzzy_query_lets_anything_stand_between_its_characters() {
        let options = SearchOptions {
            kind: SearchKind::Fuzzy,
            case_sensitive: true,
            ..options()
        };
        assert_eq!(pattern_for("a.c", &options), "a.*?\\..*?c");
    }

    #[test]
    fn a_fuzzy_query_of_one_character_stands_alone() {
        let options = SearchOptions {
            kind: SearchKind::Fuzzy,
            case_sensitive: true,
            ..options()
        };
        assert_eq!(pattern_for("a", &options), "a");
    }

    #[test]
    fn every_kind_is_offered_and_the_plainest_one_is_the_first() {
        assert_eq!(SearchKind::ALL.len(), 4);
        assert_eq!(SearchKind::ALL[0], SearchKind::default());
        for kind in SearchKind::ALL {
            assert!(
                SearchKind::ALL
                    .iter()
                    .filter(|other| **other == kind)
                    .count()
                    == 1
            );
        }
    }

    #[test]
    fn letters_of_any_alphabet_are_left_alone() {
        assert_eq!(pattern_for("кот", &options()), "(?i)кот");
    }
}
