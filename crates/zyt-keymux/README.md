# zyt-keymux

Key bindings with contexts, sequences, repeats and a command registry.

## Scope

| | |
| --- | --- |
| `KeyStroke`, `Chord` | a toolkit independent key description with a text form: `ctrl+shift+p`, `ctrl+k ctrl+s` |
| contexts | `global`, `terminal`, `tmux`, `settings`, `statusbar`, `search`, `palette`, `transfer`, and whatever else the application names |
| `Keymap` | bindings grouped by context, YAML in and out, conflict reporting, and `merge` for user overrides on top of the default layout |
| `KeyDispatcher` | holds the active context stack, collects unfinished sequences, runs a repeat, and answers `Dispatch::Command`, `Pending`, `Dropped` or `Unhandled`; `next` says what can be pressed next |
| `CommandRegistry` | registered commands, and fuzzy search for a command palette (`nucleo-matcher`) |
| `default_keymap()` | the shipped layout, built from the `DEFAULT_BINDINGS` table |

```
a key ──► KeyStroke ──► the context stack ──┬─► Command   the caller runs it
                        top context first   ├─► Pending   a sequence unfinished
                                            ├─► Dropped   it broke a sequence
                                            └─► Unhandled the caller may send bytes
```

A sequence waits for its next key without a limit. A key that goes on no
binding drops the sequence and is dropped with it, as a key after the prefix of
tmux is: it was typed for the sequence. `escape` is such a key unless bound.

A binding marked `repeat`, once run, runs again by its last key alone, and so
does every other binding of the same keys before the last marked `repeat`, until
a pause longer than the repeat time (`REPEAT_TIME`, or `set_repeat_time`). A key
that is not one of them ends the repeat and is the caller's. This is `bind -r` of
tmux: `ctrl+b ctrl+up ctrl+up ctrl+up` grows a pane three times.

```
ctrl+b   up      up     ·· pause ··   up
Pending  Command Command              Unhandled
         └─ repeat ─┘
```

A key is its place on a US keyboard, whatever the layout. `shift` alone with a
character key is the character it gives there: `shift+5` is `%`, `shift+e` is
`E` (`KeyStroke::normal`). With another modifier it stays as it is:
`ctrl+shift+c`. The caller gives the code of the key.

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
  tmux:
    - keys: ctrl+b ctrl+up
      command: tmux.pane.resize_up
      repeat: true
```

## Errors

`Result<T, KeymapError>`: `InvalidKey`, `UnknownModifier`, `EmptyBinding`,
`Conflict`, `Decode`, `Encode`. Format errors are wrapped with `#[source]`.
