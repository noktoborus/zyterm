//! What a named path amounts to: the files under it, and where each one goes.
//!
//! A symbolic link is never carried as a link. It is followed and what it
//! leads to is carried in its place, which is what someone copying a tree onto
//! a board expects and what the far end can always store. A link that leads
//! back into its own tree would make that walk endless, so both sides watch
//! for it: here the walker reports it, and on the device the real path of each
//! directory is asked for and remembered.

use crate::error::{Result, ShXferError};
use crate::session::{EntryKind, Session};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// How deep a walk of the device goes before it gives up.
const MAX_DEPTH: usize = 64;

/// A file of this machine and where it goes on the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalItem {
    /// Where the file is.
    pub path: PathBuf,
    /// Where it goes, under the directory it is carried into.
    pub relative: String,
    /// Size in bytes.
    pub size: u64,
}

/// A file of the device and where it goes on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteItem {
    /// Where the file is on the device.
    pub path: String,
    /// Where it goes, under the directory it is carried into.
    pub relative: String,
    /// Size in bytes.
    pub size: u64,
}

/// Every file under the named paths, with the tree kept.
///
/// A named file carries its own name; a named directory carries its name and
/// everything under it.
pub fn local_items(roots: &[PathBuf]) -> Result<Vec<LocalItem>> {
    let mut items = Vec::new();
    for root in roots {
        let name = file_name(root);
        let metadata = std::fs::metadata(root).map_err(|source| ShXferError::Io {
            path: root.clone(),
            source,
        })?;

        if metadata.is_file() {
            items.push(LocalItem {
                path: root.clone(),
                relative: name,
                size: metadata.len(),
            });
            continue;
        }

        for entry in walkdir::WalkDir::new(root).follow_links(true) {
            let entry = entry.map_err(|source| match source.loop_ancestor() {
                Some(ancestor) => ShXferError::Loop {
                    path: ancestor.to_path_buf(),
                },
                None => ShXferError::Walk {
                    path: source.path().unwrap_or(root).to_path_buf(),
                    source,
                },
            })?;
            if !entry.file_type().is_file() {
                continue;
            }
            let inside = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let size = entry.metadata().map(|data| data.len()).unwrap_or(0);
            items.push(LocalItem {
                path: entry.path().to_path_buf(),
                relative: join(&name, &slashed(inside)),
                size,
            });
        }
    }
    Ok(items)
}

/// Every file of the device under the named paths, with the tree kept.
///
/// A path that names nothing is left out rather than refused, so one bad name
/// among several does not stop the rest.
pub fn remote_items<W: Write>(
    session: &mut Session<W>,
    roots: &[String],
) -> Result<Vec<RemoteItem>> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for root in roots {
        let name = remote_name(root);
        match session.kind(root)? {
            EntryKind::File => items.push(RemoteItem {
                path: root.clone(),
                relative: name,
                size: session.size(root)?,
            }),
            EntryKind::Directory => walk_remote(session, root, &name, &mut seen, &mut items, 0)?,
            EntryKind::Other => log::warn!("{root} is neither a file nor a directory, left out"),
        }
    }
    Ok(items)
}

fn walk_remote<W: Write>(
    session: &mut Session<W>,
    directory: &str,
    under: &str,
    seen: &mut HashSet<String>,
    items: &mut Vec<RemoteItem>,
    depth: usize,
) -> Result<()> {
    if depth >= MAX_DEPTH {
        return Err(ShXferError::Loop {
            path: PathBuf::from(directory),
        });
    }

    let real = session.canonical(directory)?;
    if !real.is_empty() && !seen.insert(real) {
        log::warn!("{directory} was already walked, it leads back into itself");
        return Ok(());
    }

    for entry in session.list(directory)? {
        let path = join_remote(directory, &entry.name);
        let relative = join(under, &entry.name);
        match entry.kind {
            EntryKind::File => items.push(RemoteItem {
                path,
                relative,
                size: entry.size,
            }),
            EntryKind::Directory => walk_remote(session, &path, &relative, seen, items, depth + 1)?,
            EntryKind::Other => log::info!("{path} is neither a file nor a directory, left out"),
        }
    }
    Ok(())
}

/// The last part of a path, as text.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// The last part of a path of the device.
///
/// The directory the device stands in has no name of its own to carry, so what
/// is under it goes straight under the destination.
fn remote_name(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        return String::new();
    }
    trimmed.rsplit('/').next().unwrap_or(path).to_string()
}

/// A path of this machine written the way the device writes one.
fn slashed(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join("/")
}

/// Two parts of a path with one separator between them.
fn join(left: &str, right: &str) -> String {
    match (left.is_empty(), right.is_empty()) {
        (true, _) => right.to_string(),
        (_, true) => left.to_string(),
        _ => format!("{}/{}", left.trim_end_matches('/'), right),
    }
}

/// Two parts of a path of the device, with the root kept a root.
fn join_remote(directory: &str, name: &str) -> String {
    if directory == "/" {
        format!("/{name}")
    } else {
        join(directory, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(case: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("sh-xfer-walk-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the directory is made");
        path
    }

    #[test]
    fn a_named_file_carries_its_own_name() {
        let root = directory("file");
        let file = root.join("one.bin");
        std::fs::write(&file, b"abc").expect("the file is written");

        let items = local_items(std::slice::from_ref(&file)).expect("the walk works");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].relative, "one.bin");
        assert_eq!(items[0].size, 3);
    }

    #[test]
    fn a_named_directory_carries_its_tree() {
        let root = directory("tree");
        std::fs::create_dir_all(root.join("sub")).expect("the directory is made");
        std::fs::write(root.join("top.txt"), b"1").expect("the file is written");
        std::fs::write(root.join("sub/low.txt"), b"22").expect("the file is written");

        let mut items = local_items(std::slice::from_ref(&root)).expect("the walk works");
        items.sort_by(|a, b| a.relative.cmp(&b.relative));
        let name = file_name(&root);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].relative, format!("{name}/sub/low.txt"));
        assert_eq!(items[1].relative, format!("{name}/top.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_carried_as_what_it_leads_to() {
        let root = directory("link");
        std::fs::write(root.join("real.txt"), b"hello").expect("the file is written");
        std::os::unix::fs::symlink(root.join("real.txt"), root.join("as-link.txt"))
            .expect("the link is made");

        let mut items = local_items(std::slice::from_ref(&root)).expect("the walk works");
        items.sort_by(|a, b| a.relative.cmp(&b.relative));

        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.size == 5));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_that_leads_back_into_its_own_tree_is_caught() {
        let root = directory("loop");
        std::fs::create_dir_all(root.join("sub")).expect("the directory is made");
        std::os::unix::fs::symlink(&root, root.join("sub/back")).expect("the link is made");

        assert!(matches!(
            local_items(std::slice::from_ref(&root)),
            Err(ShXferError::Loop { .. })
        ));
    }

    #[test]
    fn the_parts_of_a_path_of_the_device_are_told_apart() {
        assert_eq!(remote_name("/var/log/messages"), "messages");
        assert_eq!(remote_name("."), "");
        assert_eq!(remote_name("/"), "");
        assert_eq!(join_remote("/", "etc"), "/etc");
        assert_eq!(join_remote("/var", "log"), "/var/log");
        assert_eq!(join("", "one"), "one");
    }
}
