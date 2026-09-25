# zyt-config

Configuration files in the directories the platform expects.

## Scope

- `ConfigStore::new(&AppId)` resolves the config and data directories through
  the `directories` crate: XDG base directories on Linux, Known Folders on
  Windows.
- `lock_dir` is the third, and it is not one of those: it sits under the
  temporary directory of the platform, resolved with `tempfile`. A lock says a
  copy is running *now*, which stops being true when the machine restarts, and
  a configuration directory that is synchronised between machines would carry
  stale locks to all of them. Nothing here creates it — whoever takes a lock
  does, so a copy that locks nothing leaves nothing behind.
- `load`, `load_or_create`, `save` for any serde type, YAML format
  (`serde_yaml_ng`).
- `with_paths` injects all three directories explicitly, for tests.
- A file name may name a directory of its own, `consoles/one.yaml`; `save`
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
`CreateDirectory`, `Read`, `Write`, `Decode`, `Encode`. I/O and format errors are
wrapped, never converted to text.
