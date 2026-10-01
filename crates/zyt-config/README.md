# zyt-config

Configuration files in the directories the platform expects.

## Scope

| | |
| --- | --- |
| `ConfigStore::new(&AppId)` | resolves the three directories below through the `directories` crate |
| `load`, `load_or_create`, `save` | any serde type, YAML format (`serde_yaml_ng`) |
| `with_paths` | injects all three directories explicitly, for tests |

| directory | where |
| --- | --- |
| config | XDG base directories on Linux, Known Folders on Windows |
| data | the same |
| `lock_dir` | the temporary directory of the platform, resolved with `tempfile` |

The lock directory is not one of the platform's configuration places, because a
lock says a copy is running *now*: that stops being true when the machine
restarts, and a configuration directory synchronised between machines would
carry stale locks to all of them. Nothing here creates it — whoever takes a lock
does, so a copy that locks nothing leaves nothing behind.

A file name may name a directory of its own, `consoles/one.yaml`, and `save`
creates it.

## Guarantees

- A missing file is `Ok(None)`, not an error.
- `save` writes a temporary file in the target directory and renames it, so a
  crash never leaves a half written configuration.
- `save` writes nothing when the file already says what it is being given. A
  caller says what it holds whenever it might have changed, and most of those
  times it holds what it held before; writing anyway costs a rename and a new
  modification time every one of them, which is what a program watching the
  directory sees and what a sleeping disk wakes up for.

## Errors

`Result<T, ConfigError>` with one variant per failure: `NoHomeDirectory`,
`CreateDirectory`, `Read`, `Write`, `Decode`, `Encode`. I/O and format errors
are wrapped, never converted to text.