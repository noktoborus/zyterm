# Changes since v1.0.0

What is in the working tree that was not in the `v1.0.0` tag. Written against
the two trees, not against one history: the tag and `main` share no commit, so
`git log v1.0.0..HEAD` lists every commit of `main` and answers nothing.

| | |
| --- | --- |
| from | `v1.0.0`, `ff300ca`, 2026-09-16, the project as `zyterm-x` |
| to | `a984a91`, 2026-10-08 |
| the trees differ by | 296 files, 51175 lines added, 6479 removed |
| commits on `main` | 60, the first of them a squashed base of 2026-09-25 |

The squashed base carries the work of the stretch straight after the tag and
says nothing about it in one message. Where this file names something that is
only in that base, it is marked *base*.

## Identity

Everything the program is named by changed, and nothing of the old names is read
any more.

| | at the tag | now |
| --- | --- | --- |
| package and binary | `zyterm-x` | `zyterm` |
| window title | `ZYTerm-x` | `ZYTerm` |
| desktop identity | none | `main::APP_ID`, `ru.styxheim.zyterm` |
| `AppId` the directories resolve from | `org` / `ZYTerm-x` / `ZYTerm-x` | `ru` / `styxheim` / `zyterm` |
| configuration directory, Linux | `~/.config/ZYTerm-x` | `~/.config/zyterm` |
| configuration directory, Windows | `%APPDATA%\ZYTerm-x\ZYTerm-x\config` | `%APPDATA%\styxheim\zyterm\config` |

**A tag installation keeps its files where they were and the new one does not
look there.** `keymap.yaml` and `themes/` kept their shape and are the two worth
moving by hand. The rest changed under them: a console of the tag names no `id`,
a port file is a shape the new one is not, `settings.yaml` lost six keys and
gained many, and nothing reads `profiles.yaml` at all. `ARCHITECTURE.md` has the
table of what carries the identity, and `docs/desktop.md` what the desktop does
with it.

## Serial lines and ports

| change | what it gives |
| --- | --- |
| `LineHold` — automatic, down, up | a line is held, let go, or left to the driver; two states could not say which was meant |
| `LineForce` and `PortMemory::forces` | which way a line is held is a fact about the board, written to its file |
| `BRK` as a switch | a break is a state held until it is let go, `TIOCSBRK` and `TIOCCBRK` |
| `HOLD` | the port is left unread on purpose; the bytes gather in the driver, flow control asks the device to wait |
| `flush_on_open`, `hupcl` | the driver queues are emptied once the parameters are on the line; the lines are dropped on close |
| `FlowControl::Both` | `RTS/CTS` and `XON/XOFF` at once, on a platform that holds both |
| `Settings.lines_interval` | how long the worker waits between two readings of the modem lines |
| `PortStatus` carries the driver queues | where the bytes are standing, not whether this side is done |
| `PortCommand::DiscardOutput` | gives up on what has not left the line, in all five buffers on the way |
| `RI` | the driver filled it all along and nothing ever showed it |
| `TIOCGICOUNT` | a pulse between two polls is counted, not lost |
| `PortMemory::shown_lines` | which letters a device shows, answered by that device |
| `zyt_serial::HUPCL_SUPPORTED` | the switch is not drawn where the platform has no `termios` flag |

A hold, a direction and the set of shown lines are all written to the file of
the device, so a board held in reset stays held across a replug, a reconnect and
a restart of the program.

`crates/zyt-serial` gained `comm.rs` and `tty.rs` (*base*), the two platform
halves: Linux talks to its descriptor itself, Windows through `serialport`.

## The plate of the signals

New. `docs/signals.md` is the chapter.

Resting the pointer on the button at the head of the line controls raises a
picture over the terminal: one track per signal and one per direction of the
data, filled where the signal stood. Either button pins it.

| | |
| --- | --- |
| sampled in | the port worker, beside `poll_lines`, at `lines_interval` |
| a console is sampled in | the window, `Session::sample_console`, at the same step |
| one bar is | one physical pixel, so the screen holds as much history as it has pixels |
| colours | green for a line standing up, red for `BRK` and `HOLD`, orange and yellow for the two directions |
| `TXQ` and `RXQ` | drawn as a depth against `LineScale`, the fullest each queue has been seen |
| the history outlives | a lost device, so an unplugging reads as a gap |

The plate stands against the edge the cursor is furthest from. The hints of the
six indicators are gone: the plate says for all of them at once, and over time,
what each hint said for one of them now.

## The terminal and the screen

| change | what it gives |
| --- | --- |
| a run of NUL bytes is drawn | the mark and the decimal count, in cells carrying noncharacters from `U+FDD0` |
| the mark is stroked, not a glyph | two shapes of the page, the same on every machine, sharp at any scale |
| search in the grid | `crates/zyt-term/src/search.rs` (*base*) |
| `TerminalCache` | `crates/zyt-term-egui/src/cache.rs` (*base*) |
| Alt and the wheel | the cursor keys, five steps a frame; the scrollback stays where it is |
| the read buffer floor | one kibibyte, down from sixty-four: holding the source back is what the buffer is for |
| `ByteSwap::fit` | a buffer lowered in the settings gives the memory back |
| the ladder of waits | a row that does not read as a step wears a warning; rows no longer sort under the hand |

`assets/terminal/null.sh` and `assets/terminal/random.sh` put the two halves of
the question on the screen: what the window draws itself, and what a character
no font carries comes out as.

## Selection

Reworked whole.

- It begins on the character the button went down on, not where the toolkit
  noticed the drag.
- Both ends take the whole character they stand on, so the same two cells give
  the same text in either direction.
- `Shift`+press puts the mark on the end further from the press; the selection
  cuts back when the press is inside and never turns over.
- `ctrl+shift` and an arrow picks a block out from the cursor of the device.
  Picking out is a mode: nothing is typed, the pointer belongs to the selection
  whatever a program asked for, and the status bar carries a red mark.
- The plate of the counts says the columns, the lines and the characters, in the
  corner the drag left it in.
- *save the selection* and *save the selection and open it as …* write it
  through `zyt_files::FileTask::Write`, chunked and cancellable. The path goes
  to the clipboard, which `Settings::copy_saved_path` switches off.

## The command history and the block of several lines

| change | what it gives |
| --- | --- |
| `src/history.rs` | what a shell marked with OSC 133 becomes a history (*base*) |
| `history/added.yaml` | a line of output put into the history by hand, shared by every source |
| `history::Cache` | both files read again only when the file system says they changed |
| `history::forget` | a command taken out, one step in from the command |
| the block of several lines | `ctrl+shift+enter` opens a plate with a field and a button that sends |
| `src/caret.rs` | the caret notation of `stty` — `^C`, `^[`, `^?` — read in a block |
| `SourceMemory::block_caret` | whether carets are read is a fact about the far end, so it is of the source |
| `history::Entry::caret` | an entry says what its text meant, and runs the same control code again |

## Transfers

The subject was rewritten twice, and only the last of the three shapes is in
the tree.

| | |
| --- | --- |
| at the tag | `crates/zyt-xfer`, a profile naming an external program |
| in the base | `crates/sh-xfer`, a shell protocol crate with a binary of its own |
| now | `crates/zyt-script`, a Lua engine, and the transfers as scripts under `scripts/` |

A transfer is a Lua script read at run time, not built in: it is the thing most
often changed about a device, and a change that needs a build is a change nobody
makes. `scripts/` ships `shell-transfer`, `shell-list`, `shell-driven-scp`,
`cat-file`, `xmodem`, `ymodem`, `zmodem` and `form-check`, each a directory with
a `Manifest.yaml` and an `init.lua`, plus `scripts/lib` for what they share.
`docs/scripts.md` and `scripts/README.md` are the chapters.

- No path of the device is asked for: the line is asked `pwd`, the transfer
  works below it, and every path is quoted for the shell that reads it.
- A script asks with a window of its own — ways out in a row that fits, labels
  that wrap, rows that scroll where the window ends, full window width.
- What a script is answered is kept per source (`answers/`) and per form
  (`forms/`), so one script serves every device and each device answers for
  itself.
- A running transfer offers only stopping.
- `crates/zyt-script` carries a library and a runner, which is how a script is
  tested with no window.

## The interface

| change | what it gives |
| --- | --- |
| `src/ui/connect.rs` | replaces `src/ui/ports.rs` (*base*) |
| `src/ui/icons.rs` | every icon is a code point the fonts of egui carry, with a test that asks the toolkit |
| `plate-menu` opens at any level | `MenuState::select_deep`, so a menu opens on the value in use and not on its parent |
| `searchable(false)` | an entry reached by `Right` and not by a query, for the removals and for `ports.default_source` |
| `ui::PLATE_GAP` | one constant, written three times before |
| the page of the lines | one line to a row: the switch, and what the line is and who drives it |
| the sign of trust | raises a plate of what this session may ask for |
| a *Context menu* section in the settings | the menu of the terminal and the menu of a `file://` address are one subject |
| `src/ui/mod.rs` draws the whole interface headless | and reads the shapes back for the toolkit's *Double use of widget ID* |
| `shift+Enter` in the search bar | walks the matches the other way |
| a dialog takes the full window width | a row is a label and a value, and that width is what it cannot give up |

Keys: `ctrl+shift+q` leaves the layout and egui's own `quit_shortcuts` are
emptied — `ctrl+q` is XON and is owed to the console and the port. `app.quit` is
in the palette and carries no key. A key that names a clipboard operation does
that operation and sends no byte.

## OSC

OSC 7 is `OscSettings::directory` now, honoured for trusted output and refused
for the rest. The sniffer reads sequences with a second `vte::Parser` taken from
the re-export of `alacritty_terminal`, so one stream is never read by two
versions of it; `MAX_PAYLOAD` and `memchr` went with the buffer that needed them.
`tests/osc133.rs` and `assets/osc/*` are the demonstrations (*base*).

## The desktop

New chapter, `docs/desktop.md`.

| | |
| --- | --- |
| `build.rs` | the icon resource inside `zyterm.exe`, through `winresource` |
| `assets/icon.ico` | the icon of `assets/icon.svg` at 16, 24, 32, 48, 64, 128 and 256 pixels |
| `MimeType=inode/directory` | puts the program in the *Open With* list of a file manager |
| `assets/ru.styxheim.zyterm.open.desktop` | *Open with ZYTerm* in the context menu of KDE |
| `assets/nautilus/zyterm.py` | the same item in GNOME Files, through `nautilus-python` |
| `Arguments::path` | `zyterm <path>` starts the window in that directory |
| `Makefile`, `assets/ru.styxheim.zyterm.desktop` | the install, the desktop entry and the icons (*base*) |

## Crates

| crate | |
| --- | --- |
| `crates/zyt-script` | added, the Lua engine and its runner |
| `crates/zyt-xfer` | removed |
| `crates/sh-xfer` | added in the base, removed again |

No library crate depends on another, except `zyt-term-egui`, which renders
`zyt-term`.

`experiments/glyphs`, `experiments/keys` and `experiments/flood` are added
(*base*): what the fonts of the toolkit carry, what the terminal sends for a
key, and what a terminal does as the bytes come faster. None of them is depended
on. `cargo run -p glyphs` is how an icon is picked.

## Configuration

The directory itself moved; *Identity* above says where from and where to. A
file this version cannot read is kept aside and the defaults are started from,
and no code reads what an older version wrote.

| file | change |
| --- | --- |
| `profiles.yaml` | gone; a transfer is a script with a `Manifest.yaml` of its own |
| `ports/*.yaml` | `PortMemory` is new: `line` moves out of the memory both kinds share into it, with `holds`, `forces`, `shown_lines` and `baud_rates` beside it |
| `consoles/*.yaml` | gained `id` and `directory`; `safe` is `trusted` |
| the memory both share | `transfer_profile` is `script`, with `scripts` and `variables` beside it, and `block_caret` |
| `history/` | added: the history of each source, and `added.yaml` |
| `answers/` | added: what a script is answered, per source |
| `forms/` | added: what a form is answered |

`settings.yaml` loses `font_file`, `instances`, `profiles`, `safe`, `scrollback`
and `source`. What it gains: the ladder of waits (`read_steps`, `read_above`,
`read_buffer`, `interval`), `lines_interval`, the scrollback as a memory budget
rather than a line count, the fonts of the terminal and of the interface, the OSC
switches by sequence on a trusted and an untrusted side, the shown lines by
signal, the context menu, the search options, the history keys, the ends of a
selection, the tooltip delay and the debug window.

`TransferProfile.pty` was named `hold_line` before the profiles went: the program
runs on pipes either way, and what the flag says is whether it holds the line of
the window.

## Documentation and rules

`ARCHITECTURE.md` is an index — the crates, the threads, the data path, the
identity, the errors — and every other subject is a file of `docs/`, named in the
table at its head. The ten chapters of `docs/` are all new since the tag. Every
crate README carries scope, boundaries and errors in that order. A mapping is a
table and a sequence of states is a drawn diagram.

`AGENTS.md` gained twenty-five rules. The ones that decide the most: `app.id`,
`config.legacy` and `config.default`, `experiments`, `format` and `format.tool`,
`icon.glyph` and `icon.search`, `script.resource` and `script.product`,
`license`, `commit` and `commit.clean`, and the twelve `doc.*` rules this file is
written under.

`llms.txt` is gone. So is `src/instance.rs`: every running copy held a numbered
slot, taken with a file lock, and nothing asks for the number any more.

## Dependencies

Added to `[workspace.dependencies]`: `base64`, `clap`, `crossterm`, `fontdb`,
`known-folders`, `libc`, `md-5`, `memmap2`, `mlua`, `ratatui`,
`raw-window-handle`, `rustix`, `sha1`, `sha2`, `system-fonts`, `tempfile`,
`uuid`, `walkdir`, `windows`, `winresource`, `xdg`.

Removed: `memchr`.

## Release

`.github/workflows/release.yml` builds Linux x86-64, Linux ARM64 and Windows
x86-64. A tag `v*` makes a release of the archives; a run started by hand keeps
them as artifacts. `assets/terminal/nul.sh` is `null.sh`, because the former
name cannot be cloned on Windows.
