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
pub fn search(items: &[MenuItem], query: &str) -> Vec<Flat> {
    let flat = flatten(items);
    if query.trim().is_empty() {
        return flat;
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buffer = Vec::new();

    let mut scored: Vec<(u32, Flat)> = flat
        .into_iter()
        .filter_map(|entry| {
            let item = item_at(items, &entry.path)?;
            let mut haystack = match entry.parents.is_empty() {
                true => item.label.clone(),
                false => format!("{} {}", entry.parents, item.label),
            };
            if !item.search.is_empty() {
                haystack.push(' ');
                haystack.push_str(&item.search);
            }
            let score = pattern.score(Utf32Str::new(&haystack, &mut buffer), &mut matcher)?;
            Some((score, entry))
        })
        .collect();

    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, entry)| entry).collect()
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
    fn an_empty_query_keeps_every_entry() {
        assert_eq!(search(&menu(), "  ").len(), flatten(&menu()).len());
    }
}
