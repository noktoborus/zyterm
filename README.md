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
  held down, or held up. The letters stand pressed while the line is driven from
  here, and their colour is what the line is doing. A forced state is written
  down for the device and put back on the line every time the port opens, so a
  board held in reset stays in reset across a replug, across a disconnect and
  across a restart of this program. Left to the driver, nothing is written to the
  line at all.
- `BRK` stands ahead of them and holds the line in the break condition, which is
  a state and not a key: it lasts until it is let go, because that is what a
  device reading a break as a request for attention waits for. It survives a
  replug the way a forced hold does, and the palette carries it as *toggle
  BREAK*. It leads the row because a held break is a line carrying no byte at
  all, which is what explains every reading beside it going quiet.
- `HOLD` beside it stops the port being read, which is the same state the read
  buffer reaches by itself when a window cannot keep up, asked for on purpose.
  What it does to the device is the flow control's business: with it the bytes
  gather in the driver and the device is told to wait, and `RTS` on the plate
  falls for exactly as long; without it the device is told nothing and what the
  driver cannot hold is lost. The hint says which of the two the line is on, and
  the palette carries it as *toggle HOLD*.
- Resting the pointer on any of those letters raises the plate of the signals
  over the terminal — `CTS`, `DSR`, `DCD` and `RI` beside the ones this side
  drives; the right button on any of them leaves it standing, and the
  palette carries the same switch as *toggle the plate of the signals*, because a
  line watched while both hands are typing cannot be a line watched by holding a
  pointer still. It shows: one track per line and one for each direction of the data,
  filled where the signal stood and empty where it did not, what this side drives
  above what the device does. It answers the one question a serial line always
  raises — did it go quiet by itself, or did a signal stop it — which a row of
  letters saying what is true *now* never could. It stands against the edge the
  terminal cursor is furthest from, so the rows being written into are the rows
  it never covers. One bar is one reading of the lines, so how far back the plate
  reaches follows how often they are read, and the span is written under it. The names of the lines written out, which those
  letters used to show on hover, are gone: the plate is what the pointer finds
  there instead.
- Which of those letters a device shows is its own setting, and the plate draws
  the same set: the row and the tracks are the same signals read two ways. Six of
  the eight stand to begin with — a `DCD` tied high by an adapter and an `RI` wired
  to nothing are noise in a row read at a glance and flat tracks in a picture read
  for the one that is not flat, so a device that uses them says so.
- The settings of one port carry two things that happen when nobody is watching:
  whether the driver buffers are emptied as the port opens, so a session does not
  begin in the middle of a sentence nobody asked for, and `HUP` — whether the
  driver drops `DTR` and `RTS` as the port closes, which is how a device is told
  the session ended: a modem hangs up, a board wired to reset restarts. `HUP` is a
  `termios` flag, so it is offered only where the platform has one; on Windows
  what happens to the lines on close is the driver's business and the setting is
  not drawn at all.
- How often the modem lines are read is a setting, because every reading is a
  call into the driver on the thread that reads the port: a line watched closely
  is a line read less.
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
- A run of NUL bytes is drawn as a block — a red frame, the colours of the cell
  exchanged, a mark, and `×` with the count where the run was longer than one
  byte. A standard terminal drops the byte, so a device that went quiet in the
  middle of a word leaves nothing behind anywhere else, and that nothing is what
  somebody watching a line came to see. The mark is drawn by the window rather
  than taken from a font, the way Firefox draws the character no font carries, so
  it is the same on every machine; the count is text, because digits have to read
  as part of the line. `assets/terminal/nul.sh` puts every case of it on the
  screen, and `assets/terminal/random.sh` fills the page with characters instead —
  which is where a box says nothing in the chain of fonts carries one.
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
