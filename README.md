# ZYTerm

GUI terminal for serial consoles and the local shell. Linux and Windows.

## Goals

- A terminal that stays quick and answers at once on a small Linux machine. A
  Raspberry Pi 4 is what it is measured against, not a desktop.
- Take the awkwardness out of working in a TUI program, which is what a device
  at the end of a serial line usually puts in front of a person.
- A better interactive session than a plain terminal gives, on this machine and
  on the one at the far end.

Version 1.0.0, the first release.

## Keys

| key | what it does |
| --- | --- |
| `ctrl+shift+o` | show the menu of the sources; nothing is disconnected until one is chosen |
| `ctrl+shift+n` | another window on the same console, as a new process |
| `ctrl+shift+tab` | hand the keyboard between the terminal and the status bar |
| `ctrl+shift+f` | search the screen and the scrollback; `Enter` steps up, `ctrl+f` down |
| `ctrl+shift+r` | the commands the shell marked |
| `ctrl+shift+c` / `v` | copy, paste |
| `ctrl+shift` held | take the mouse from a program reading it, or hand it back |

`ctrl+f` stays free, so `^F` still reaches the device. Bindings, contexts and
sequences are YAML in the config directory, and every command is in the palette.

## Sources

- Serial ports and local consoles are one list: a menu of plates showing the
  path or the program, the name of the device, and when it was last opened.
  Typing searches it by either. It is ordered by last use, read again once a
  second while it stands, and ports the process cannot open are counted but not
  listed.
- A device is followed by USB identity, not by path, so one that comes back as
  `/dev/ttyUSB1` is the same device. Unplugging releases the handle at once.
- `RTS` and `DTR` in the status bar open three states each: left to the driver,
  held down, or held up. A forced state is put back on the line every time the
  port opens, so a board held in reset stays in reset across a replug. The
  letters stand pressed while the line is driven from here, and their colour is
  what the line is doing.
- A console is a command line, not a shell script: `ssh -p 2222 host` is split
  the way a shell splits it and run without one. Consoles are files under
  `consoles/`, identified by an id that renaming does not touch.
- A console may name the directory it starts in, and may be told to start again
  when its program ends with code 0.
- A console whose command line names a value it has no answer for asks before it
  opens — `ssh {remote_host}` — and the answers are kept per source, so the same
  board is `Enter` and another one is the difference typed over it.
- A console that ends any other way, and a source that cannot be opened at all,
  ask what to do: connect again, choose another, or close. The output stays on
  the screen while it asks.

## Terminal

The operating system commands the terminal answers. The *OSC support* settings
decide which of them are honoured; a sequence they refuse is dropped without an
answer, so a program asking for more than it may have is left waiting for
nothing.

| sequence | what it says | how it is answered |
| --- | --- | --- |
| `OSC-0`, `OSC-2` | the window title | The window wears it while *Window title* is honoured. A program that resets it gives the window the name of the application back. |
| `OSC-4`, `OSC-10`, `OSC-11`, `OSC-12` | the colours a program paints itself in | `4` paints over one entry of the 256 colour table, `10` and `11` move the default pair, `12` names the cursor. All four behind *Palette*, and `104`, `110`, `111` and `112` give a colour back. A colour is read as `rgb:rr/gg/bb` or `#rrggbb`, never as a name. |
| `OSC-7` | the working directory | It becomes the directory of the process, which is where the file dialog and a transfer start. It has no switch of its own. |
| `OSC-8` | a hyperlink | Drawn as a link while *Links* is honoured. A `file://` address opens the menu of that file instead of the browser. |
| `OSC-9` | a notification with a text | Handed to the desktop while *Notification* is honoured. The heading is the name of the application, since the sequence carries none. |
| `OSC-9;4` | how far along a program is | A bar left of the status bar while *Progress* is honoured, and the taskbar on Windows. All five states are read; a share above a hundred is brought back to it. |
| `OSC-52` | the clipboard | Four settings rather than a switch, from refusing everything to copy and paste. Reading the clipboard is answered from what the program stored itself at the setting between them. |
| `OSC-133` | the marks around a prompt and a command | `A`, `B`, `C` and `D` with the exit code. What stands between `B` and `C` is the command, which is what fills the history. Honoured behind *Shell marks*. |
| `OSC-777` | a notification with a heading | Handed to the desktop while *Notification with a heading* is honoured. Only the `notify` form is read. |

`assets/osc` holds a script per sequence, played once from a console, so what a
switch does can be seen.

What a press of the left button does, by the keys held with it. A program that
asks for the mouse gets every button of it, and what is on the screen under one
— a table, a column of numbers, a log with a prefix — is still text somebody
needs, so there are two ways past it.

| keys held | what a press does |
| --- | --- |
| none | Begins a selection and grows it while the button is down. One press selects cell by cell, two a whole word, three a whole line. |
| `shift` | Moves the end of the selection that stands, so one press grows it and the next shrinks it. |
| `ctrl` | Selects a rectangle rather than a run of text. Only a single press asks for one: a rectangle of words is not a thing to select. |
| `ctrl+shift` | Turns the mouse grab the other way for as long as it is held, so a press selects a run of text in a program that is reading the mouse — and reports to that program while it is not. |
| `ctrl+shift+alt` | Selects a rectangle in a program that is reading the mouse. The press is not reported at all, whichever way the grab stands. |

The rest of the terminal:

- Search over the screen and the scrollback: four ways of reading the query,
  case and highlight switches, the current match as the selection.
- Scrollback is a memory budget rather than a line count, because a row costs
  the full width of the window. A change takes hold without a restart.
- A held finger selects the word under it and adds what it moves over.
- The right button held and moved scrolls the page; released in place it opens
  the menu.
- The middle button pastes the platform selection.
- Terminal palettes in the Alacritty colour format, read from `themes/` of the
  configuration, one for the dark mode and one for the light. `assets/themes`
  holds two examples.
- Fonts are the ones the system has. The terminal takes a chain of monospaced
  families, the interface takes one family of any kind, and what the toolkit
  ships with stays behind both, so a glyph nothing chosen carries is still drawn.

## Files

- A `file://` link printed by a console gets a menu of its own: open,
  open with, rename, move here, copy the address, the text or the contents, to
  the trash, or delete. Each runs on a thread of its own and can be stopped, and
  the settings decide which entries exist and which ask first.
- A file dropped onto the window opens the menu of that file: send it, insert
  its path, insert what it holds, or pick another transfer profile first. X11,
  Windows and macOS deliver drops; Wayland does not.
- The context menu picks a file or a directory from the dialog and types its
  path in, quoted the way a shell reads it.

## File transfer

- A profile is a pair of command lines with delays, or a single program that
  runs beside the line. Placeholders carry what the transfer moves (`{>file}`),
  the parts of its name (`{:stem}`), and the values the connection keeps
  (`{remote_host}`).
- The shipped profiles are the modems, `cat`, `sh-xfer` and `SCP to remote PWD`,
  which asks the device for `pwd` and lets `scp` do the rest.
- A transfer on the line holds the keyboard and is stopped from over the
  terminal. It counts as finished only once the last byte left the port.
- `sh-xfer` carries files to a device with nothing installed on it: a plain
  shell on the far end and FISH on the wire, base64 or raw with `stty`, chunks
  that each say where they belong, and `--digest auto` to compare sums on both
  sides. `crates/sh-xfer/PROTOCOL.md` is the wire format.
- Transfers and file operations stand in one panel with the time each has been
  running and the button that stops it.

## Build and run

```sh
cargo run
```

The window is created twice: normally first, then in a new process with software
rendering forced, if the first attempt fails.

| command | what it does |
| --- | --- |
| `make build` | the two release binaries, for working on the program |
| `make pgo` | the same, profile guided, through `cargo-pgo` |
| `make install` | the *profiled* binaries, desktop entry, icons and `osc133-bash.sh` under `/usr/local` |
| `make uninstall` | takes them back out |

`PREFIX` and `DESTDIR` say where. Installing never builds: it takes what
`make pgo` left and refuses when there is none, so the release path is
`make pgo && make install`.

The desktop entry and the icons are installed under the application identity,
`ru.styxheim.zyterm`, which is also the `app_id` the window carries. The two
have to agree or the desktop cannot tell that the window is this program.

`make pgo` builds the workspace instrumented, runs the test suite as the
workload, merges the profiles with `llvm-profdata` (`rustup component add
llvm-tools-preview`) and builds the binaries again. The workload is allowed to
fail — a red test still measured something. To profile the program itself,
`cargo pgo run -- --bin zyterm` and then `cargo pgo optimize build -- --bin
zyterm`; `cargo pgo clean` throws the profiles away.

`osc133-bash.sh` is installed because it is the only script meant to be kept:
sourcing it makes bash mark its commands, which is what fills the history. The
rest of `assets/osc` is played once from a console and run out of the source
tree.

## Configuration

Platform configuration directory, named by the application identity —
`~/.config/zyterm` on Linux, `styxheim\zyterm` under Known Folders on Windows:

| path | holds |
| --- | --- |
| `settings.yaml` | the interface, the performance settings, what a program may ask for |
| `consoles/` | one file per console, named by its identity |
| `ports/` | one file per device: line parameters, what it remembers |
| `history/` | commands a shell marked, one file per source |
| `answers/` | values typed before a console was opened, one file per source |
| `profiles.yaml` | transfer profiles |
| `keymap.yaml` | key bindings, written on first start |
| `themes/` | terminal palettes in the Alacritty colour format |
| `file-dialog.yaml` | what the file dialog remembers |

A file this version cannot read is kept aside and the defaults are started from.
A key a file does not carry reads as the default of that setting, which is how a
setting added later reaches a file written without it.

## Workspace

| Crate | Purpose |
|---|---|
| `crates/zyt-serial` | ports, line parameters, hot plug supervision |
| `crates/zyt-pty` | local console |
| `crates/zyt-term` | terminal emulation over a byte stream |
| `crates/zyt-term-egui` | egui widget for the terminal |
| `crates/zyt-keymux` | key bindings, contexts, command registry |
| `crates/plate-menu` | the menu widget every list is drawn with |
| `crates/zyt-xfer` | external transfer programs over pipes |
| `crates/zyt-files` | file operations on their own threads |
| `crates/zyt-config` | configuration directories and YAML files |
| `crates/sh-xfer` | FISH over a console line, library and command line |

`experiments/*` are tools, not parts of the program: `glyphs` lists what the
fonts of the toolkit carry, `keys` writes down what a terminal sends for a key,
`flood` fills standard output faster and faster.

Each crate has its own README. `ARCHITECTURE.md` is the data flow and the
decisions; `AGENTS.md` is the rules the code follows.

## License

MIT, the whole workspace. `LICENSE` carries the text; every crate declares it
with `license.workspace = true`.
