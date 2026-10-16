//! Searching a menu, entries and sub-entries alike.

use crate::item::MenuItem;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// One entry of the tree, addressed by the indices that lead to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flat {
    /// Indices from the root down to this entry.
    pub path: Vec<usize>,
    /// Labels of the entries above it, joined for display.
    pub parents: String,
    /// Characters of `parents` the query matched, by index, ascending.
    pub matched_parents: Vec<usize>,
    /// Characters of the label the query matched, by index, ascending.
    pub matched_label: Vec<usize>,
}

/// Every entry that can be chosen, the whole tree deep.
///
/// A parent is listed as well when it can be chosen on its own; a separator
/// never is.
pub fn flatten(items: &[MenuItem]) -> Vec<Flat> {
    let mut found = Vec::new();
    walk(items, &mut Vec::new(), &mut Vec::new(), &mut found);
    found
}

/// Entries matching the query, the best first.
///
/// The query is matched against the label of the entry together with the labels
/// above it, so a sub-entry is found by its own name and by the name of what
/// carries it — and together with whatever else the entry says it is known by
/// (`MenuItem::search`), which is drawn nowhere.
///
/// An entry that says it is not searchable is left out whatever the query is,
/// the empty one included: it is reached by walking into the entry it belongs
/// to and nowhere else.
pub fn search(items: &[MenuItem], query: &str) -> Vec<Flat> {
    let flat: Vec<Flat> = flatten(items)
        .into_iter()
        .filter(|entry| item_at(items, &entry.path).is_some_and(|item| item.searchable))
        .collect();
    if query.trim().is_empty() {
        return flat;
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);

    let mut scored: Vec<(u32, Flat)> = flat
        .into_iter()
        .filter_map(|mut entry| {
            let item = item_at(items, &entry.path)?;
            let label_from = match entry.parents.is_empty() {
                true => 0,
                false => entry.parents.chars().count() + 1,
            };
            let mut haystack = match entry.parents.is_empty() {
                true => item.label.clone(),
                false => format!("{} {}", entry.parents, item.label),
            };
            if !item.search.is_empty() {
                haystack.push(' ');
                haystack.push_str(&item.search);
            }

            // One character is one index: the matched ones are drawn, and a
            // grapheme of several characters would move every index after it.
            let characters: Vec<char> = haystack.chars().collect();
            let mut indices = Vec::new();
            let score =
                pattern.indices(Utf32Str::Unicode(&characters), &mut matcher, &mut indices)?;
            indices.sort_unstable();
            indices.dedup();

            let label_to = label_from + item.label.chars().count();
            for index in indices.into_iter().map(|index| index as usize) {
                if index + 1 < label_from {
                    entry.matched_parents.push(index);
                } else if (label_from..label_to).contains(&index) {
                    entry.matched_label.push(index - label_from);
                }
            }
            Some((score, entry))
        })
        .collect();

    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, entry)| entry).collect()
}

/// Indices leading to the entry with this identifier, the whole tree deep.
///
/// The levels are looked through in the order they are drawn, so an identifier
/// two entries carry is answered by the first of them — the one a reader walking
/// the menu reaches first. An entry with no identifier is passed over: nothing
/// names it.
pub fn path_of(items: &[MenuItem], id: &str) -> Option<Vec<usize>> {
    for (index, item) in items.iter().enumerate() {
        if !item.id.is_empty() && item.id == id {
            return Some(vec![index]);
        }
        if let Some(mut path) = path_of(&item.children, id) {
            path.insert(0, index);
            return Some(path);
        }
    }
    None
}

/// Entry the indices lead to.
pub fn item_at<'a>(items: &'a [MenuItem], path: &[usize]) -> Option<&'a MenuItem> {
    let (first, rest) = path.split_first()?;
    let item = items.get(*first)?;
    if rest.is_empty() {
        Some(item)
    } else {
        item_at(&item.children, rest)
    }
}

/// Entries one step below the indices, the root for an empty path.
pub fn level<'a>(items: &'a [MenuItem], path: &[usize]) -> &'a [MenuItem] {
    match item_at(items, path) {
        Some(item) => &item.children,
        None => items,
    }
}

fn walk(items: &[MenuItem], path: &mut Vec<usize>, names: &mut Vec<String>, out: &mut Vec<Flat>) {
    for (index, item) in items.iter().enumerate() {
        if item.separator {
            continue;
        }
        path.push(index);
        if item.enabled {
            out.push(Flat {
                path: path.clone(),
                parents: names.join(" / "),
                matched_parents: Vec::new(),
                matched_label: Vec::new(),
            });
        }
        if item.has_children() {
            names.push(item.label.clone());
            walk(&item.children, path, names, out);
            names.pop();
        }
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Vec<MenuItem> {
        vec![
            MenuItem::new("copy", "Copy"),
            MenuItem::separator(),
            MenuItem::new("profile", "Cat file").children(vec![
                MenuItem::new("profile.zmodem", "zmodem"),
                MenuItem::new("profile.xmodem", "xmodem"),
            ]),
        ]
    }

    #[test]
    fn every_entry_of_the_tree_is_reachable() {
        let flat = flatten(&menu());
        let paths: Vec<Vec<usize>> = flat.into_iter().map(|entry| entry.path).collect();

        assert_eq!(paths, vec![vec![0], vec![2], vec![2, 0], vec![2, 1]]);
    }

    #[test]
    fn a_sub_entry_is_found_by_its_own_name_and_by_its_parent() {
        let menu = menu();

        let hits = search(&menu, "xmod");
        assert_eq!(
            hits.first().map(|entry| entry.path.clone()),
            Some(vec![2, 1])
        );
        assert_eq!(hits[0].parents, "Cat file");

        let hits = search(&menu, "cat");
        assert!(
            hits.iter().any(|entry| entry.path == vec![2, 0]),
            "the children of a matching parent are offered too"
        );
    }

    /// An entry is found by what it says it is known by as readily as by its
    /// label, and none of that is drawn: a device is looked for by the path it
    /// is at and by the name of the thing plugged in there.
    #[test]
    fn an_entry_is_found_by_what_it_says_it_is_known_by() {
        let menu = vec![
            MenuItem::new("usb", "/dev/ttyUSB0").search("FTDI FT232R"),
            MenuItem::new("plain", "/dev/ttyS0"),
        ];

        let hits = search(&menu, "ft232");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, vec![0]);

        let hits = search(&menu, "ttyS0");
        assert_eq!(
            hits.first().map(|entry| entry.path.clone()),
            Some(vec![1]),
            "the label still finds it first"
        );
    }

    #[test]
    fn an_entry_is_found_by_its_identifier_at_any_depth() {
        let menu = menu();

        assert_eq!(path_of(&menu, "copy"), Some(vec![0]));
        assert_eq!(path_of(&menu, "profile.xmodem"), Some(vec![2, 1]));
        assert_eq!(path_of(&menu, "nothing of the sort"), None);
    }

    /// A separator and a parent that names nothing carry no identifier, and an
    /// empty one is not an entry to look for.
    #[test]
    fn an_entry_with_no_identifier_is_not_found() {
        assert_eq!(path_of(&menu(), ""), None);
    }

    /// An entry that is about the entry above it is walked into and never
    /// found by typing: on its own it says nothing about which entry it acts on.
    #[test]
    fn an_entry_that_is_not_searchable_is_no_hit_of_any_query() {
        let menu = vec![
            MenuItem::new("history:make", "make")
                .choosable(true)
                .children(vec![
                    MenuItem::new("history.forget:make", "Remove").searchable(false),
                ]),
        ];

        assert_eq!(
            search(&menu, "make")
                .into_iter()
                .map(|entry| entry.path)
                .collect::<Vec<Vec<usize>>>(),
            vec![vec![0]],
            "the command is found and the entry below it is not"
        );
        assert!(search(&menu, "remove").is_empty());
        assert_eq!(search(&menu, "").len(), 1, "nor by an empty query");
        assert_eq!(
            path_of(&menu, "history.forget:make"),
            Some(vec![0, 0]),
            "it is still the entry it was, and is walked into"
        );
    }

    /// The characters a query matched are told apart in the parents and in the
    /// label; what matched in the text drawn nowhere is not told at all.
    #[test]
    fn the_characters_a_query_matched_are_told() {
        let menu = vec![
            MenuItem::new("usb", "ttyUSB0").search("FTDI"),
            MenuItem::new("port", "Port").children(vec![MenuItem::new("acm", "ttyACM0")]),
        ];

        let hits = search(&menu, "usb");
        assert_eq!(hits[0].path, vec![0]);
        assert_eq!(hits[0].matched_label, vec![3, 4, 5]);
        assert!(hits[0].matched_parents.is_empty());

        let hits = search(&menu, "pacm");
        assert_eq!(hits[0].path, vec![1, 0]);
        assert_eq!(hits[0].matched_parents, vec![0], "the P of Port");
        assert_eq!(hits[0].matched_label, vec![3, 4, 5], "the ACM of ttyACM0");

        let hits = search(&menu, "ftdi");
        assert_eq!(hits[0].path, vec![0]);
        assert!(
            hits[0].matched_label.is_empty() && hits[0].matched_parents.is_empty(),
            "a match in what is drawn nowhere marks nothing"
        );
    }

    #[test]
    fn an_empty_query_keeps_every_entry() {
        assert_eq!(search(&menu(), "  ").len(), flatten(&menu()).len());
    }
}
