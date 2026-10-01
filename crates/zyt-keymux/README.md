# zyt-keymux

Key bindings with contexts, sequences and a command registry.

## Scope

| | |
| --- | --- |
| `KeyStroke`, `Chord` | a toolkit independent key description with a text form: `ctrl+shift+p`, `ctrl+k ctrl+s` |
| contexts | `global`, `terminal`, `settings`, `statusbar`, `search`, `palette`, and whatever else the application names |
| `Keymap` | bindings grouped by context, YAML in and out, conflict reporting, and `merge` for user overrides on top of the default layout |
| `KeyDispatcher` | holds the active context stack, collects unfinished sequences, and answers `Dispatch::Command`, `Pending` or `Unhandled` |
| `CommandRegistry` | registered commands, and fuzzy search for a command palette (`nucleo-matcher`) |
| `default_keymap()` | the shipped layout, built from the `DEFAULT_BINDINGS` table |

```
a key ──► KeyStroke ──► the context stack ──┬─► Command   the caller runs it
                        top context first   ├─► Pending   a sequence unfinished
                                            └─► Unhandled the caller may send bytes
```

## Boundaries

The crate takes keys and returns command identifiers. It runs nothing, draws
nothing and depends on no toolkit. Titles shown in the palette are passed in by
the caller, already translated, so the crate holds no localized text.

## File format

```yaml
contexts:
  global:
    - keys: ctrl+shift+p
      command: palette.open
  terminal:
    - keys: ctrl+k ctrl+s
      command: keymap.edit
```

## Errors

`Result<T, KeymapError>`: `InvalidKey`, `UnknownModifier`, `EmptyBinding`,
`Conflict`, `Decode`, `Encode`. Format errors are wrapped with `#[source]`.
