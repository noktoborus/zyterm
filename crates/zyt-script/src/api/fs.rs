//! `zyt.fs`: the files of this machine.
//!
//! Enough to carry a file and no more: a script opens, reads at an offset,
//! writes, asks what a path is and walks a tree. The walk is here rather than
//! in Lua because it follows links and has to notice one that leads back into
//! the tree it stands in, which is an endless walk and a transfer that never
//! finishes.

use super::external;
use crate::error::ScriptError;
use mlua::{Lua, Table, UserData, UserDataMethods};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How deep a walk of this machine goes before it gives up.
const MAX_DEPTH: usize = 64;

/// One file a script opened.
#[derive(Debug)]
struct Opened {
    path: PathBuf,
    file: Mutex<Option<std::fs::File>>,
}

impl Opened {
    /// The error of a call on this file.
    fn failed(&self, source: std::io::Error) -> mlua::Error {
        external(ScriptError::Io {
            path: self.path.clone(),
            source,
        })
    }

    /// Does one thing with the file, or says it is closed.
    fn with<T>(
        &self,
        body: impl FnOnce(&mut std::fs::File) -> std::io::Result<T>,
    ) -> mlua::Result<T> {
        let mut slot = self
            .file
            .lock()
            .map_err(|_| self.failed(std::io::Error::other("the file is in use")))?;
        let file = slot
            .as_mut()
            .ok_or_else(|| self.failed(std::io::Error::other("the file is closed")))?;
        body(file).map_err(|source| self.failed(source))
    }
}

impl UserData for Opened {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("read", |lua, this, count: Option<usize>| {
            let mut bytes = Vec::new();
            match count {
                Some(count) => {
                    let mut buffer = vec![0u8; count];
                    let read = this.with(|file| file.read(&mut buffer))?;
                    buffer.truncate(read);
                    bytes = buffer;
                }
                None => {
                    this.with(|file| file.read_to_end(&mut bytes))?;
                }
            }
            if bytes.is_empty() {
                return Ok(mlua::Value::Nil);
            }
            Ok(mlua::Value::String(lua.create_string(&bytes)?))
        });

        methods.add_method("write", |_, this, data: mlua::LuaString| {
            let bytes = data.as_bytes().to_vec();
            this.with(|file| file.write_all(&bytes))?;
            Ok(())
        });

        methods.add_method("seek", |_, this, at: u64| {
            this.with(|file| file.seek(SeekFrom::Start(at)))
        });

        methods.add_method("size", |_, this, ()| {
            this.with(|file| file.metadata().map(|data| data.len()))
        });

        methods.add_method("flush", |_, this, ()| this.with(|file| file.flush()));

        methods.add_method("close", |_, this, ()| {
            if let Ok(mut slot) = this.file.lock() {
                let _ = slot.take();
            }
            Ok(())
        });

        methods.add_method("path", |_, this, ()| {
            Ok(this.path.to_string_lossy().to_string())
        });
    }
}

/// Builds the table.
pub(crate) fn table(lua: &Lua) -> mlua::Result<Table> {
    let fs = lua.create_table()?;

    fs.set(
        "open",
        lua.create_function(|_, (path, how): (String, Option<String>)| {
            let path = PathBuf::from(path);
            let how = how.unwrap_or_else(|| "r".to_string());
            let mut options = std::fs::OpenOptions::new();
            match how.as_str() {
                "r" => options.read(true),
                "w" => options.write(true).create(true).truncate(true),
                "a" => options.append(true).create(true),
                "rw" => options.read(true).write(true).create(true),
                other => {
                    return Err(external(ScriptError::Io {
                        path,
                        source: std::io::Error::other(format!("no file is opened {other}")),
                    }));
                }
            };
            let file = options.open(&path).map_err(|source| {
                external(ScriptError::Io {
                    path: path.clone(),
                    source,
                })
            })?;
            Ok(Opened {
                path,
                file: Mutex::new(Some(file)),
            })
        })?,
    )?;

    fs.set(
        "stat",
        lua.create_function(|lua, path: String| {
            let path = PathBuf::from(path);
            let Ok(data) = std::fs::metadata(&path) else {
                return Ok(mlua::Value::Nil);
            };
            let stat = lua.create_table()?;
            stat.set(
                "kind",
                if data.is_dir() {
                    "directory"
                } else if data.is_file() {
                    "file"
                } else {
                    "other"
                },
            )?;
            stat.set("size", data.len())?;
            Ok(mlua::Value::Table(stat))
        })?,
    )?;

    fs.set(
        "list",
        lua.create_function(|lua, path: String| {
            let path = PathBuf::from(path);
            let mut names: Vec<String> = std::fs::read_dir(&path)
                .map_err(|source| {
                    external(ScriptError::Io {
                        path: path.clone(),
                        source,
                    })
                })?
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .collect();
            names.sort_unstable();
            lua.create_sequence_from(names)
        })?,
    )?;

    fs.set(
        "mkdir",
        lua.create_function(|_, path: String| {
            let path = PathBuf::from(path);
            std::fs::create_dir_all(&path).map_err(|source| {
                external(ScriptError::Io {
                    path: path.clone(),
                    source,
                })
            })
        })?,
    )?;

    fs.set(
        "remove",
        lua.create_function(|_, path: String| {
            let path = PathBuf::from(path);
            std::fs::remove_file(&path).map_err(|source| {
                external(ScriptError::Io {
                    path: path.clone(),
                    source,
                })
            })
        })?,
    )?;

    fs.set(
        "rename",
        lua.create_function(|_, (from, to): (String, String)| {
            let from = PathBuf::from(from);
            std::fs::rename(&from, PathBuf::from(to)).map_err(|source| {
                external(ScriptError::Io {
                    path: from.clone(),
                    source,
                })
            })
        })?,
    )?;

    fs.set(
        "temp_file",
        lua.create_function(|_, (prefix, suffix): (Option<String>, Option<String>)| {
            let file = tempfile::Builder::new()
                .prefix(&prefix.unwrap_or_else(|| "zyt-script-".to_string()))
                .suffix(&suffix.unwrap_or_else(|| ".tmp".to_string()))
                .tempfile()
                .map_err(|source| {
                    external(ScriptError::Io {
                        path: std::env::temp_dir(),
                        source,
                    })
                })?;
            let (_, path) = file.keep().map_err(|error| {
                external(ScriptError::Io {
                    path: error.file.path().to_path_buf(),
                    source: error.error,
                })
            })?;
            Ok(path.to_string_lossy().to_string())
        })?,
    )?;

    fs.set(
        "absolute",
        lua.create_function(|_, path: String| {
            let path = PathBuf::from(path);
            let whole = std::path::absolute(&path).map_err(|source| {
                external(ScriptError::Io {
                    path: path.clone(),
                    source,
                })
            })?;
            Ok(whole.to_string_lossy().to_string())
        })?,
    )?;

    fs.set(
        "url",
        lua.create_function(|_, path: String| Ok(url_of(Path::new(&path))))?,
    )?;

    fs.set(
        "walk",
        lua.create_function(|lua, roots: Vec<String>| {
            let roots: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).collect();
            let items = walk(&roots).map_err(external)?;
            let listed = lua.create_table()?;
            for (at, item) in items.iter().enumerate() {
                let entry = lua.create_table()?;
                entry.set("path", item.path.to_string_lossy().to_string())?;
                entry.set("relative", item.relative.clone())?;
                entry.set("size", item.size)?;
                listed.set(at + 1, entry)?;
            }
            Ok(listed)
        })?,
    )?;

    Ok(fs)
}

/// One file a walk found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Walked {
    /// Where it stands on this machine.
    pub path: PathBuf,
    /// Where it stands under the root it was found in, with forward slashes.
    pub relative: String,
    /// How big it is.
    pub size: u64,
}

/// Every file under the named paths, with where each one belongs.
///
/// A link is followed and what it leads to is carried in its place, which is
/// what someone copying a tree onto a board expects. A link leading back into
/// its own tree is refused by name rather than walked for ever.
pub(crate) fn walk(roots: &[PathBuf]) -> crate::Result<Vec<Walked>> {
    let mut items = Vec::new();

    for root in roots {
        let named = name_of(root);
        let walking = walkdir::WalkDir::new(root)
            .follow_links(true)
            .max_depth(MAX_DEPTH);

        for entry in walking {
            let entry = entry.map_err(|source| match source.loop_ancestor() {
                Some(path) => ScriptError::Loop {
                    path: path.to_path_buf(),
                },
                None => ScriptError::Walk {
                    path: root.clone(),
                    source,
                },
            })?;
            if !entry.file_type().is_file() {
                continue;
            }

            let size = entry.metadata().map(|data| data.len()).unwrap_or(0);
            let under = entry.path().strip_prefix(root).ok();
            let relative = match under {
                Some(rest) if rest.as_os_str().is_empty() => named.clone(),
                Some(rest) => format!("{named}/{}", slashed(rest)),
                None => named.clone(),
            };
            items.push(Walked {
                path: entry.path().to_path_buf(),
                relative,
                size,
            });
        }
    }

    Ok(items)
}

/// The name of a path, without the directories above it.
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// A relative path with forward slashes, which is what the far end reads.
fn slashed(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join("/")
}

/// The address of a file, for a terminal that draws links.
fn url_of(path: &Path) -> String {
    let whole = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut text = String::from("file://");
    for part in slashed(&whole).split('/') {
        if part.is_empty() {
            continue;
        }
        text.push('/');
        for byte in part.as_bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    text.push(*byte as char)
                }
                other => text.push_str(&format!("%{other:02X}")),
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test, with a file in it.
    fn tree(case: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("zyt-script-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("under")).expect("the tree is made");
        std::fs::write(root.join("one.txt"), b"one").expect("a file is written");
        std::fs::write(root.join("under/two.txt"), b"two!").expect("a file is written");
        root
    }

    #[test]
    fn a_walk_says_where_every_file_belongs() {
        let root = tree("walk");
        let mut found = walk(std::slice::from_ref(&root)).expect("it walks");
        found.sort_by(|one, two| one.relative.cmp(&two.relative));

        let named: Vec<(String, u64)> = found
            .iter()
            .map(|item| (item.relative.clone(), item.size))
            .collect();
        let name = root.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(
            named,
            vec![
                (format!("{name}/one.txt"), 3),
                (format!("{name}/under/two.txt"), 4)
            ]
        );

        std::fs::remove_dir_all(&root).expect("it is taken away");
    }

    #[test]
    fn a_link_that_leads_back_into_its_own_tree_is_refused_by_name() {
        #[cfg(unix)]
        {
            let root = tree("loop");
            std::os::unix::fs::symlink(&root, root.join("under/back")).expect("the link is made");

            let error = walk(std::slice::from_ref(&root)).expect_err("the walk is refused");
            assert!(matches!(error, ScriptError::Loop { .. }), "{error:?}");

            std::fs::remove_dir_all(&root).expect("it is taken away");
        }
    }

    #[test]
    fn an_address_of_a_file_is_escaped() {
        let url = url_of(Path::new("/tmp/two words/it's here.txt"));
        assert!(url.starts_with("file:///"), "{url}");
        assert!(url.contains("two%20words"), "{url}");
        assert!(url.contains("it%27s"), "{url}");
    }
}
