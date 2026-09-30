//! The operations themselves, each one able to stop where it is.

use crate::error::{FileError, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Bytes moved between two checks of the cancel flag.
const CHUNK: usize = 64 * 1024;

/// Told how far an operation has come: bytes done, bytes expected.
pub type Progress<'a> = &'a mut dyn FnMut(u64, Option<u64>);

/// Moves a file or a directory, by renaming it where that works and by copying
/// it where it does not.
///
/// A rename cannot cross a file system, so the copy is the path that carries
/// the progress and can be stopped; what it half wrote is removed again.
pub fn move_path(
    from: &Path,
    to: &Path,
    cancel: &Arc<AtomicBool>,
    progress: Progress<'_>,
) -> Result<PathBuf> {
    if stopped(cancel) {
        return Err(FileError::Cancelled);
    }

    let size = size_of(from).unwrap_or(0);
    if std::fs::rename(from, to).is_ok() {
        progress(size, Some(size));
        return Ok(to.to_path_buf());
    }

    if from.is_dir() {
        let mut done = 0;
        copy_tree(from, to, cancel, progress, &mut done)?;
        delete_path(from, cancel, progress)?;
        return Ok(to.to_path_buf());
    }

    copy_file(from, to, cancel, progress)?;
    std::fs::remove_file(from).map_err(|source| FileError::Io {
        path: from.to_path_buf(),
        source,
    })?;
    Ok(to.to_path_buf())
}

/// Copies a directory with everything under it, one file at a time.
///
/// How much there is to copy is not asked for beforehand: walking a tree twice
/// costs more than it tells, so the progress counts what is done and leaves the
/// total unknown.
fn copy_tree(
    from: &Path,
    to: &Path,
    cancel: &Arc<AtomicBool>,
    progress: Progress<'_>,
    done: &mut u64,
) -> Result<()> {
    std::fs::create_dir_all(to).map_err(|source| FileError::Io {
        path: to.to_path_buf(),
        source,
    })?;

    let entries = std::fs::read_dir(from).map_err(|source| FileError::Io {
        path: from.to_path_buf(),
        source,
    })?;
    for entry in entries {
        if stopped(cancel) {
            return Err(FileError::Cancelled);
        }
        let entry = entry.map_err(|source| FileError::Io {
            path: from.to_path_buf(),
            source,
        })?;
        let source = entry.path();
        let target = to.join(entry.file_name());

        if source.is_dir() {
            copy_tree(&source, &target, cancel, progress, done)?;
        } else {
            let mut carried = 0;
            copy_file(&source, &target, cancel, &mut |bytes, _| carried = bytes)?;
            *done += carried;
            progress(*done, None);
        }
    }
    Ok(())
}

/// Copies a file in chunks, stopping where it is asked to.
pub fn copy_file(
    from: &Path,
    to: &Path,
    cancel: &Arc<AtomicBool>,
    progress: Progress<'_>,
) -> Result<PathBuf> {
    let total = size_of(from);
    let mut source = std::fs::File::open(from).map_err(|source| FileError::Io {
        path: from.to_path_buf(),
        source,
    })?;
    let mut target = std::fs::File::create(to).map_err(|source| FileError::Io {
        path: to.to_path_buf(),
        source,
    })?;

    let mut buffer = vec![0_u8; CHUNK];
    let mut done = 0_u64;
    loop {
        if stopped(cancel) {
            drop(target);
            let _ = std::fs::remove_file(to);
            return Err(FileError::Cancelled);
        }

        let read = source.read(&mut buffer).map_err(|source| FileError::Io {
            path: from.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        target
            .write_all(&buffer[..read])
            .map_err(|source| FileError::Io {
                path: to.to_path_buf(),
                source,
            })?;
        done += read as u64;
        progress(done, total);
    }

    target.flush().map_err(|source| FileError::Io {
        path: to.to_path_buf(),
        source,
    })?;
    Ok(to.to_path_buf())
}

/// Writes bytes into a file, stopping where it is asked to.
///
/// The file is written in chunks and not in one call, for the same reason a copy
/// is: the flag is looked at between two of them, so a save of something large
/// can be stopped. What a stopped one half wrote is removed again — a file that
/// holds the first half of what was asked for is worse than no file, because
/// nothing about it says which half it is.
///
/// An existing file is replaced. Where the bytes are going is a question already
/// answered by whoever picked the path.
pub fn write_file(
    path: &Path,
    bytes: &[u8],
    cancel: &Arc<AtomicBool>,
    progress: Progress<'_>,
) -> Result<PathBuf> {
    let total = Some(bytes.len() as u64);
    let mut target = std::fs::File::create(path).map_err(|source| FileError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let mut done = 0_u64;
    for chunk in bytes.chunks(CHUNK) {
        if stopped(cancel) {
            drop(target);
            let _ = std::fs::remove_file(path);
            return Err(FileError::Cancelled);
        }
        target.write_all(chunk).map_err(|source| FileError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        done += chunk.len() as u64;
        progress(done, total);
    }

    target.flush().map_err(|source| FileError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    progress(done, total);
    Ok(path.to_path_buf())
}

/// Hands a file to the trash of the desktop.
pub fn trash_file(path: &Path, cancel: &Arc<AtomicBool>) -> Result<()> {
    if stopped(cancel) {
        return Err(FileError::Cancelled);
    }
    trash::delete(path).map_err(|source| FileError::Trash {
        path: path.to_path_buf(),
        source,
    })
}

/// Removes a file, or a directory with everything under it, for good.
///
/// A directory is walked and emptied one entry at a time, so the cancel flag is
/// answered between entries; what was already removed stays removed, which is
/// what stopping a deletion can mean. A link is removed itself and never
/// followed: what it points at is not what was asked for.
pub fn delete_path(path: &Path, cancel: &Arc<AtomicBool>, progress: Progress<'_>) -> Result<()> {
    let mut done = 0;
    delete_tree(path, cancel, progress, &mut done)
}

fn delete_tree(
    path: &Path,
    cancel: &Arc<AtomicBool>,
    progress: Progress<'_>,
    done: &mut u64,
) -> Result<()> {
    if stopped(cancel) {
        return Err(FileError::Cancelled);
    }

    let about = std::fs::symlink_metadata(path).map_err(|source| FileError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    if about.is_dir() {
        let entries = std::fs::read_dir(path).map_err(|source| FileError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| FileError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            delete_tree(&entry.path(), cancel, progress, done)?;
        }
        std::fs::remove_dir(path).map_err(|source| FileError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    } else {
        std::fs::remove_file(path).map_err(|source| FileError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    }

    *done += 1;
    progress(*done, None);
    Ok(())
}

/// Reads a whole file, refusing one larger than `limit`.
///
/// The size is asked for before anything is read, so a file too large costs
/// nothing but the question.
pub fn read_file(
    path: &Path,
    limit: u64,
    cancel: &Arc<AtomicBool>,
    progress: Progress<'_>,
) -> Result<Vec<u8>> {
    let size = size_of(path).unwrap_or(0);
    if size > limit {
        return Err(FileError::TooLarge {
            path: path.to_path_buf(),
            size,
            limit,
        });
    }

    let mut file = std::fs::File::open(path).map_err(|source| FileError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::with_capacity(size as usize);
    let mut buffer = vec![0_u8; CHUNK];

    loop {
        if stopped(cancel) {
            return Err(FileError::Cancelled);
        }
        let read = file.read(&mut buffer).map_err(|source| FileError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() as u64 > limit {
            return Err(FileError::TooLarge {
                path: path.to_path_buf(),
                size: bytes.len() as u64,
                limit,
            });
        }
        progress(bytes.len() as u64, Some(size));
    }

    Ok(bytes)
}

/// Size of a file, when the system will say.
///
/// A directory has no size worth reporting: what it holds would have to be
/// walked, which costs more than the number is worth.
pub fn size_of(path: &Path) -> Option<u64> {
    let data = std::fs::metadata(path).ok()?;
    data.is_file().then_some(data.len())
}

fn stopped(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(case: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("zyt-files-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the directory is created");
        path
    }

    fn nothing() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn a_file_is_moved_and_read_back() {
        let directory = directory("move");
        let from = directory.join("one.txt");
        let to = directory.join("two.txt");
        std::fs::write(&from, b"payload").expect("the file is written");

        let mut seen = (0, None);
        move_path(&from, &to, &nothing(), &mut |done, total| {
            seen = (done, total)
        })
        .expect("the file is moved");

        assert!(!from.exists());
        assert_eq!(seen.0, 7);
        assert_eq!(
            read_file(&to, 1024, &nothing(), &mut |_, _| {}).expect("the file is read"),
            b"payload"
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_file_larger_than_the_limit_is_refused_before_it_is_read() {
        let directory = directory("limit");
        let path = directory.join("big.bin");
        std::fs::write(&path, vec![0_u8; 4096]).expect("the file is written");

        let refused = read_file(&path, 1024, &nothing(), &mut |_, _| {});
        assert!(matches!(
            refused,
            Err(FileError::TooLarge {
                size: 4096,
                limit: 1024,
                ..
            })
        ));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_deletion_stops_between_entries() {
        let directory = directory("delete-cancel");
        let tree = directory.join("tree");
        std::fs::create_dir_all(&tree).expect("the tree is made");
        for number in 0..8 {
            std::fs::write(tree.join(format!("{number}.txt")), b"x").expect("written");
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let stopped = delete_path(&tree, &cancel, &mut |done, _| {
            if done == 3 {
                flag.store(true, Ordering::Relaxed);
            }
        });

        assert!(matches!(stopped, Err(FileError::Cancelled)));
        assert!(tree.exists(), "what was not reached is still there");
        assert_eq!(
            std::fs::read_dir(&tree)
                .expect("the tree is readable")
                .count(),
            5,
            "and what was removed stays removed"
        );

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_cancelled_copy_leaves_nothing_behind() {
        let directory = directory("cancel");
        let from = directory.join("one.bin");
        let to = directory.join("two.bin");
        std::fs::write(&from, vec![7_u8; 4096]).expect("the file is written");

        let cancel = Arc::new(AtomicBool::new(true));
        let stopped = copy_file(&from, &to, &cancel, &mut |_, _| {});

        assert!(matches!(stopped, Err(FileError::Cancelled)));
        assert!(!to.exists(), "the half written file is gone");
        assert!(from.exists(), "and the file it was copied from is not");

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_file_is_deleted() {
        let directory = directory("delete");
        let path = directory.join("one.txt");
        std::fs::write(&path, b"x").expect("the file is written");

        delete_path(&path, &nothing(), &mut |_, _| {}).expect("the file is deleted");
        assert!(!path.exists());

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_directory_goes_with_everything_under_it() {
        let directory = directory("delete-tree");
        let tree = directory.join("tree");
        std::fs::create_dir_all(tree.join("below/deeper")).expect("the tree is made");
        std::fs::write(tree.join("one.txt"), b"x").expect("the file is written");
        std::fs::write(tree.join("below/two.txt"), b"x").expect("the file is written");
        std::fs::write(tree.join("below/deeper/three.txt"), b"x").expect("the file is written");

        let mut removed = 0;
        delete_path(&tree, &nothing(), &mut |done, _| removed = done)
            .expect("the directory is deleted");

        assert!(!tree.exists());
        assert_eq!(removed, 6, "three files and three directories");

        let _ = std::fs::remove_dir_all(directory);
    }
}
