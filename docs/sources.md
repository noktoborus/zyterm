# Sources

A source is a serial port or a console. Both are addressed by a `SourceKey`:

```
SourceKey ─┬─ Console(ConsoleId)     identity, survives renaming
           └─ Port(PortId)           usb:vid:pid:serial, or path:/dev/…
```

The text form names files and never collides, because a device key starts with
its kind and a console identity does not.

| what is remembered | where |
| --- | --- |
| a console: name, program, args, palettes, trust, restart | `consoles/<id>.yaml` |
| both kinds: last connection, transfer directory, script, offered scripts, variables (`SourceMemory`) | in the console file, or `ports/<key>.yaml` |
| a device: line parameters, offered speeds (`PortMemory`) | `ports/<key>.yaml` |
| commands the shell marked | `history/<key>.yaml` |
| values typed into the ask window | `answers/<key>.yaml` |
| what a script was answered | `forms/<key>-<script>-<form>.yaml` |

Nothing is shared: the parameters of one device never reach another, and
`Settings.line` is only what a device that was never used starts with. A console
file carries nothing of a line, and nothing of a line can reach one —
`App::set_line_params` asks the session which *device* it is on.

Three empty values mean something:

| | |
| --- | --- |
| `scripts` empty | every script |
| `baud_rates` empty | the shared list |
| `variables` empty | a source that answers no name |

`variables` is a list, not a map, because it is edited a letter at a time.

`ConfigStore::save` encodes, compares with the file and returns without writing
when they match. A write that changes nothing still costs a rename and a new
mtime.

## The source menu

A scan of the ports reads the file system, so it runs only while the source menu
stands, once a second (`App::list_sources_again`, `PlateMenu::refill`). The menu
lists the ports the process may open — `PortInfo::accessible` — and counts the
rest. What a scan finds is `crates/zyt-serial/README.md`.

`ctrl+shift+o` (`port.choose`) shows the menu and nothing else: choosing is what
ends a connection, since `connect_serial` and `connect_console` disconnect
before they open. `port.disconnect` is a menu entry with no key, because it
cannot be taken back.

## Consoles

A console is its `ConsoleId`. The name is what it is called and may change at
any time, so the file, what is remembered, the answers, the history, the default
source and `--console` all point at the identity.

`consoles::shipped` is what the program comes with: the shell of this machine,
and `SSH`, which names `{remote_user}` and `{remote_host}` rather than carrying
them. `ssh` is run with `-e none`, so it keeps no escape character out of the
stream, and `-o NumberOfPasswordPrompts=1`, so a wrong password ends the console
instead of asking twice more; it is not trusted. `add_shipped` writes at startup
the ones the directory has never seen, recognised by identity: a renamed one is
not written again, a deleted one comes back.

| field | what it does |
| --- | --- |
| `program` | one line read with `shlex`, so `ssh -p 2222 host` is a program and three words; nothing is run through a shell, and an unclosed quote is an error |
| `directory` | the working directory of the pty; empty, the console starts where the application stands |
| `restart` | reopens the console when it ends with code 0 — a log follower, a board that reboots |

Anything other than code 0 asks instead, because a program that cannot start
fails again at once.

### Ending

```
exit code 0, no restart  →  the window has no source, shows the last screen
exit code 0, restart     →  the same console is opened again
anything else            →  App.lost: connect again / choose another / close
cannot be opened at all   →  the same question
```

`App.lost` holds what is asked about and why; `App::ask_about_lost` opens the
menu with the failure as its notice. `LostReason::Ended` carries the exit code,
and no code at all — a program that closed its handles and stayed — is not an
ordinary end either. `Esc` is answered as *choose another*, because a question
waved away must not close the window. The failure is printed in the terminal
too.

## Windows

A window is a copy of the program; nothing is shared between two of them.
`ctrl+shift+n` starts another with argv[0] and `--console <id>`, in the current
directory. What a window opens is settled once at startup: `--console`, else
`Settings.default_source`, else nothing and the source menu.

## What is asked before a connection

A console whose command line, arguments or directory name a value that its own
settings do not answer puts up `ui::ask` — one row per name — and is opened only
once it is answered. It is asked every time, because a name left out of the
settings is the answer to "which one this time".

While the window stands, what is behind it answers nothing: `Ui::disable` for
the pointer, and `App::handle_keyboard` reads no key, as behind a menu. `Enter`
walks the fields and lands on the button that connects; the cross and `Esc` lead
back to the source list with nothing connected.

It grows with the names it asks for, the way the window a script asks with
does, and is never narrower than the row that ends it: the two ways on are laid
out by what each of them takes, and a window asking for one short name is
narrower than their own words.

Answers are kept in `answers/<key>.yaml`. They are not `SourceMemory.variables`:
a value there is a decision about the console, an answer here is what it was
pointed at this evening. `answers::fill` puts them into a *clone* of the console
being opened, only where its own memory is empty, on every path into a console.

They are also what the connection answers by name afterwards. `App::answered`
keeps them per source for the run, and `App::variables_of` is the one place
that reads a source: the settings of it, filled with what was typed on the way
in. That is what a script is given, what the menu of the scripts is built from
and what the warning of the settings page is decided by — a script asking for
`remote_host` runs on a connection that was told one, instead of being left out
until the value is written into the settings as well.

`Console.name` carries values too, so what a console is addressed by and what it
is shown as differ: `Console::key` is the name as written,
`Console::display_name` has the values put in, and `Session::console_shown`
keeps what was shown from the moment it started. Nothing is quoted there, a
console not being run through a shell, and a name with no value is left
standing.

## Command history

Shell marks (OSC 133) say where a command begins and where its output does. The
line itself is in neither, so `zyt-term` reads it out of the grid and hands it
over as `TerminalEvent::Command`. Recording at `133;C` means the exit code plays
no part — a command that failed is exactly the one somebody wants back.

`App::write_down_commands` drains them each frame into `history/<key>.yaml`.
`history::List` says which file a call is about:

```
List ─┬─ Source(SourceKey)   history/<key>.yaml, the commands that source marked
      └─ Added               history/added.yaml, added by hand, shared by all
```

The shared file is the same shape and carries no `key`. It is what the *Add to
the command history* entry of a selection writes to: a line worth keeping was
worth keeping wherever it was read, and the source it was read on is often not
the one it is to be typed into.

Three rules:

1. **Read again on every look.** Several copies may share a console, so nothing
   is kept between two looks. `history::Cache` asks the file system when the file
   was last written and how long it is, and parses it again only when that answer
   changed. `Cache::changed` covers the one write that answer cannot tell apart —
   a list at its limit that dropped a command for one of the same length, inside
   whatever the file system counts a moment — because a write this copy made is
   known without asking.
2. **Changed under a lock.** `ConfigStore::save` is atomic within one process
   only, so `history::remember` and `history::forget` take a blocking
   `File::lock` on `history/<slug>.lock` for the whole read-change-write.
3. **Kept once.** An equal entry is removed before the new one goes to the
   front, carrying the directory and the moment of this run.
   `Settings.command_history` caps the count, in the shared file as in the others.

`ctrl+shift+r` opens the list: both files in one, ordered by when each command
last ran, because which file a command is kept in is not how anybody looks for
it. A plate is one line — line breaks become spaces, longer than
`HISTORY_LENGTH` characters is cut with an ellipsis — and the whole command, its
directory and its last run stand on the plate beside the menu. The button is
drawn only while one of the files has something in it, asked by file size rather
than by parsing, because it is asked every frame.

Every command carries *Remove* one step in, `choosable`, so `Enter` still types
the command back and `Right` is the way to that entry. The entry carries the
whole identifier of the command, which says the file to take it out of.
`history::forget` takes the file away when it empties it, because file size is
what the button asks about. The menu is opened again afterwards, since the widget
closes on a choice and commands are often taken out one after another.

`Settings.command_history_keys` says what each way of choosing does — `Enter`
runs the command, `Shift+Enter` leaves it standing, unless turned over. Running
means a carriage return, the way every line this program types into a device is
closed. Which key chose it arrives as `Chosen.held`.

Nothing here is ever reported to the user: a history is a convenience, so a file
that cannot be read, written or locked leaves a log line and an empty list.
