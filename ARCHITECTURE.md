# Architecture

## Crates

```
zyterm (binary: eframe, rust-i18n, dark-light, egui-file-dialog, arboard,
          fontdb, system-fonts, memmap2)
├── zyt-serial    libc (linux), serialport (windows)
├── zyt-pty       portable-pty
├── zyt-term      alacritty_terminal
├── zyt-term-egui egui + zyt-term
├── zyt-keymux    serde_yaml_ng + nucleo-matcher
├── plate-menu    egui + nucleo-matcher
├── zyt-xfer      std::process
├── zyt-files     trash + mime_guess
└── zyt-config    directories + serde_yaml_ng
```

No library crate depends on another, except `zyt-term-egui`, which renders
`zyt-term`. Only the binary knows all of them and only the binary holds user
facing text.

## Threads

```
ui thread        draws, reads settings, owns Session and Terminal
port worker      one per open port: opens, reads, writes, polls modem lines
pty read/write   one pair per console
transfer         one per running transfer or file task
theme watcher    waits for the desktop colour change signal
```

Workers never touch the terminal. They fill buffers and send events; the ui
thread drains both once per frame.

## Data path

Payload bytes never travel through a channel.

```
port/pty thread          ByteSwap (Mutex<Vec<u8>>)        ui thread
  read what fits ──push──►  filled   ──take_into──►  spare ──► Terminal::feed
  write_all     ◄─take_into─ outgoing ◄──push───── key input, terminal answers
```

`take_into` clears the caller buffer and swaps it with the shared one: one lock
and one pointer swap per frame, and no allocation at all once both buffers stand
at their size. `zyt-pty` and `zyt-xfer` use the same type.

Control information — connection state, modem lines, errors, transfer progress —
travels as messages (`PortEvent`, `TransferEvent`), because it is small and rare.

### Read buffer

`Settings.read_buffer` (kibibytes) is the size of the filled buffer. It is
allocated once at that size and never grows, so a source that fills it stops
being read instead:

```
buffer with room  →  read what fits (ByteSwap::room), push, wake the window
buffer full       →  ByteSwap::wait_for_room, no read at all
```

The worker reads `room()` bytes and no more, which is what keeps a chunk from
carrying the buffer past its size. `push` takes only what fits and says how much
that was, so a caller that read more than the room keeps the rest rather than
growing the buffer with it. The spare the ui thread hands back is brought up to
the size before it is filled again: one allocation per buffer, at the first swap
or when the setting changes, and none after that.

Nothing is dropped. The bytes wait where they were produced: in the pty pipe,
whose program blocks on its next `write`, and in the port driver, which asserts
flow control if the line has any. A line without flow control loses what the
driver cannot hold — the same loss such a line always has.

The wait times out and is taken again, so a worker that is no longer wanted can
notice: the pty thread checks `running`, the port worker answers its commands
and polls modem lines while held back.

### The ladder of waits

How long the bytes wait before the ui thread takes them is not one number but a
ladder, because a console being typed into and a console pouring out a build log
are not the same thing. `Settings.read_steps` is a list of
`ReadStep { speed, interval }`: a row holds up to its speed in kibibytes a
second, and the slowest row the source has not passed is the one in force.

| up to | wait |
| --- | --- |
| 1 KiB/s | none, taken whenever a frame asks |
| 12 KiB/s | 27 ms |
| 40 KiB/s | 60 ms |
| above | grows with the speed, up to 1 s |

A source can always be faster than the ladder goes, so `Settings.read_above`
(`ReadAbove { interval, linear }`) says what happens there. With `linear` the
wait grows in step with the speed — twice the speed of the last row is twice its
wait — and stops at `interval`; the two meet at that row, so nothing jumps where
the ladder ends. Without it, `interval` stands from the first byte past the
ladder. A last row that waits for nothing has nothing to grow from, and neither
has an empty ladder, so `interval` stands there too.

`Session::byte_rate` counts what arrived in the last second (`crate::rate`), and
`App::apply_read_interval` reads the ladder with it on every frame
(`config::read_wait`). The window of numbers shows both the speed and the wait,
so a row can be set against what it does.

A wait holds back the reading and nothing else: the interface is drawn whenever
the toolkit draws it. A transfer lifts the wait, since its protocol answers block
by block.

## Port state machine

```
Disconnected ──found + open ok──► Connected
     ▲                               │
     │                     read/write/line error
     └──────── handle dropped ◄──────┘
```

- A fatal error drops the handle at once. A held handle keeps the device node
  claimed, so a returning device would appear under a new name.
- The target is a `PortId`: USB vendor, product and serial when reported, path
  otherwise. `/dev/ttyUSB1` after a replug is still the same device.
- While disconnected the worker rescans every `scan_interval`.
- `write_some` reports how much the driver took and the worker keeps the rest;
  `write_all` would discard a partially written block on a timeout, which on a
  binary transfer means a corrupt block.
- Modem lines are polled every `lines_interval` (250 ms). On unix `TIOCMGET`
  answers all six at once, including RTS and DTR, which the port crate does not
  expose. Windows cannot read those two back, so they are `None` and only what
  was asked for is known.

## Terminal

`zyt-term` drives `alacritty_terminal::Term` through `vte::ansi::Processor`. No
process is involved, so the same emulator serves a serial line, a console or a
test buffer. `render_into` writes a snapshot of the visible page in the crate's
own types; `take_output` returns answers to device queries, which the
application writes back to the line.

Scrollback is a memory budget, not a line count. A row is kept at the full width
of the window whatever stands in it (`GRID_CELL_BYTES` per cell), so ten
thousand lines cost 19 MiB at 80 columns and 94 MiB at 400.
`Settings.scrollback_memory` is in mebibytes; `config::scrollback_lines` turns
it into lines at the current width, and `App::apply_scrollback` hands that to
the grid every frame, because every frame is where the width may have changed.
`Terminal::set_scrollback` drops what is above the new cap at once.

`Terminal::forget_scrollback` drops the lines above the screen and keeps the
cap. `App::follow_connection` calls it when a session ends.

The parser and the OSC sniffer walk a chunk together. `OscSniffer::feed`
answers `SniffedReport` with the offset where each sequence ended, and
`Terminal::feed` advances the parser to that offset before acting on the report.
This matters for shell marks only: a mark says where it stands in the output,
which is true only while the bytes before it are drawn and the bytes after are
not. A chunk with no such sequence is one `advance`.

## Rendering

```
Terminal ──render_into──► RenderableContent ──build_picture──► Mesh ──► egui
             (visible page only)              (cached between frames)
```

Only the visible page is ever rendered. Scrollback lives in the grid; scrolling
changes `display_offset` and the next snapshot covers the new page.

`TerminalCache` keeps the grid as one mesh. It is rebuilt when the page differs
from the previous one, or when the area, cell size, scale, font, theme, atlas
size or the `program_colors`/`links` flags differ (`Painted`).

- `read_page` renders into a spare buffer and compares it with the page the
  caller holds. A touched terminal is not a changed page: a key press, a drag
  over the same cells, a row repainted with the same text. The comparison is
  whole-page — the picture is one mesh, so finding *where* it differs buys
  nothing — and the two buffers are then swapped, never copied.
- The cursor is not in the mesh. It moves with nearly every byte and blinks, so
  it is drawn over the mesh every frame. Its shape, including the hollow one of
  an unfocused window, costs no rebuild.
- The link under the pointer is drawn over the mesh too.
- Each cell is drawn centred in its own rectangle, one character at a time. The
  toolkit rounds the glyph cursor to the pixel grid when laying out a string, so
  a laid-out row drifts off the cell grid.
- Shapes are built first and tessellated afterwards: laying out a character is
  what grows the atlas, and a mesh names glyphs as a share of the atlas size.
- The mesh buffer of the previous picture is reused, so vertices are allocated
  once and not once per picture.

The glyph atlas is what the cache rests on: a mesh names each glyph as a share
of the atlas image, so an atlas that grew moves all of them.

| event | when the picture is rebuilt |
| --- | --- |
| page changed | same pass |
| palette changed | pass of its own (`App.theme_settling`) |
| fonts replaced | pass of its own (`App.fonts_settling`) |
| atlas grew | next pass (`App::watch_the_atlas`) |

Fonts take effect only in the pass after they are handed over, and a palette
change happens mid-pass, so both defer the rebuild rather than cut the picture
against an atlas that is about to move.

`App::raw_input_hook` caps the atlas side at `render::ATLAS_SIDE`. epaint
doubles the atlas height as glyphs arrive, so every step costs the full width,
in memory and on the chip.

`wgpu` reaches the chip through Vulkan, DirectX or OpenGL. `src/render.rs` picks
the adapter by hand — a power preference never rules out a software adapter —
and cuts the requested limits down to what the adapter offers, because the
`wgpu` defaults exceed what a small chip such as the Raspberry Pi V3D grants.
Texture sizes are asked for in full. A machine with no chip still gets a window
and the log says so.

## Frames and idle cost

The window repaints only when there is a reason. Device output, connection
events and transfer progress wake it through the `notify` callback of the
workers; an idle terminal asks for no frames.

| what ticks | interval |
| --- | --- |
| the menu of the sources, while open | 1 s |
| a running transfer | 200 ms |
| the plate of the times, while open | 10 s |
| the window of numbers, while open | 500 ms |
| a running file task | slow tick, for its elapsed time |

`notify` calls `request_repaint_after(read_interval)` rather than
`request_repaint`, so a hundred arrivals inside one interval wake the window
once. The interval is the one the ladder of `Settings.read_steps` answers at the
speed of the moment, and it holds back the reading itself as well.

The desktop colour change arrives on a `dark_light` channel, which nobody would
be waiting on in an idle window, so a thread of its own marks it and asks for a
repaint. `ThemeWatcher::poll` then asks the platform again on the ui thread, so
the colour is decided in one place.

Waiting for the toolkit to answer a clipboard request is the one place that
would spin, so it asks for a frame each time and gives up after 400 ms. Writer
threads wait on condition variables rather than polling.

## Startup

Nothing that can wait is done before the window shows.

```
main ──► Settings, keymap, consoles ──► window ──► first frame ──► port scan
```

Reading the font directories is the longest such job and is only done when a
family was chosen. The port scan waits for the frame that shows the window:
`App::ui` marks the window as drawn and the scan runs on the next frame.

`main` starts the window normally. If that fails it re-execs the program with
`LIBGL_ALWAYS_SOFTWARE`, `GALLIUM_DRIVER=llvmpipe` and `WGPU_BACKEND=gl`,
because the toolkit allows one event loop per process.

### Identity

`main::APP_ID` is `ru.styxheim.zyterm` and is the one name the desktop knows the
program by.

| carries it | as |
| --- | --- |
| the window | `app_id` on Wayland, `WM_CLASS` on X11 |
| the desktop entry | its file name, and `StartupWMClass` inside it |
| the icon | its file name under `share/icons/hicolor` |
| the Makefile | `ID`, which installs all three |

A window whose `app_id` names no desktop entry cannot be matched to the program:
the desktop draws it with the default icon and lists it a second time. The
window title (`main::WINDOW_TITLE`) is a different thing — it is read by a
person and changes with the session.

The configuration directory is resolved from the same identity through
`directories`, which asks for the application name alone on Linux and for the
organization as well on Windows.

## Main area

Three views replace each other; none is a separate window, and each gives the
keyboard its own binding context.

```
┌──────────────────────────────────────┐
│ terminal | settings | file dialog    │  main area
├──────────────────────────────────────┤
│ status bar, or the search bar        │
└──────────────────────────────────────┘
   menu of plates and the plate of the times float above
```

The terminal is shown whenever the other two are not, connected or not: a window
whose session ended still holds what was on the screen.

The status bar carries controls only, never a message. Left to right: the
connection label, then signs — trust, mouse grab, transfer holding the stream —
then pending output, running tasks, command history, search, gear. In the
settings it carries only the way back and the gear, since everything else there
names a connection the settings may not be showing.

The search bar replaces the status bar and is shown even when that one is
switched off. It owns the keyboard while it stands (`ui.focus` is never
`Terminal` then), takes the terminal selection as its initial query, and closes
on a click in the terminal. Its four query kinds are a menu of plates and are
kept in the settings. `zyt-term` holds the pattern; a whole-word match is
decided by the cells beside a match, because the regex word boundary is rejected
by the engine alacritty builds and an ascii one would be blind to every alphabet
but latin.

The file dialog is `egui-file-dialog`, sized to the main area each frame,
without a title bar, keeping a hundredth of the window free along each edge.
What it remembers is `<config>/file-dialog.yaml`, read on every opening and
written when a path is picked, so two copies of the program do not overwrite
each other.

Pages scroll with `widgets::scroll_area`, which takes the scrollbar out of the
input while a finger is down: a press on the track is a jump, and a swipe that
began on the bar threw the page wherever the finger went.

## Selection and pointer

- A selection begins *before* the character it was started on; where it ends,
  the half of the cell the pointer is on decides.
- A press that selects nothing leaves an anchor there. It is a place in the
  text, so it moves with the text. Drawing a bar at it is a setting, off by
  default.
- `Shift`+press moves the end of the selection; the start stays. A link is not
  opened while `Shift` is held.
- Press count decides the unit: one cell, two words, three lines. It is counted
  on the press, because the toolkit decides a double click on the release, after
  the drag has begun.
- `Ctrl` on a single press selects a rectangle. `Ctrl+Shift+Alt` does too and
  takes the press from a program reading the mouse. That `Alt` is read from the
  key as well as from the modifier state, because `AltGr` is not reported as
  `Alt` on layouts that type a second alphabet.
- A held finger selects the word under it and adds what it moves over. The
  toolkit reports a held finger where it reports a right click and takes the
  drag away, so the menu uses `clicked_by(Secondary)` and the selection uses
  `long_touched` and the pointer position.
- The right button held and moved scrolls the page; released in place it still
  opens the menu. Movement is accumulated in points and spent in whole lines.

The view follows the end of the output only while it already stands there.
Dragging the scrollbar to the bottom, scrolling there, or typing puts it back.
`Session::follows_output` is asked before application notices are printed too.

## The menu

One widget draws every menu: the terminal menu, the session menu, a link menu,
the command palette, the command history, the sources, and every list to choose
from — a list is a menu with the entry in use marked.

`plate-menu` knows `egui` and `nucleo-matcher` and nothing else. Entries are
plain data built in `src/ui/menu.rs`; a choice comes back as `plate_menu::Chosen`
— the identifier and the modifiers held on that frame.

| identifier | meaning |
| --- | --- |
| a command id | run it |
| `link.*` | act on the link under the pointer |
| `profile:<name>` | pick a transfer profile |
| `history:<command>` | type a command back |
| `source:<kind>:<name>` | open a source |
| `choice:<list>:<slot>:<value>` | a value of the settings |

Keys: arrows and wheel walk, `Right` steps in, `Left` steps out or clears the
query, `Enter` chooses, `Esc` closes. An entry with children is stepped into
unless it says `choosable`, and then it is chosen and `Right` is the way in.
Typing searches the whole tree at once and shows hits flat; `MenuItem::search`
adds text that is matched but not drawn, which is how a port is found by the
name of the device plugged into it. Nothing scrolls: only the plates that fit
are drawn and the window of plates walks with the selection.

`MenuItem::full` carries the whole of an entry that was cut to one line. It is
drawn on a plate right of the menu, which shifts left so the two keep the middle
of the window. `PlateMenu::shift` says whether that room is kept while the
selection walks. `PlateMenu::refill` replaces the entries of an open menu and
keeps the query, level and selection, which is what the source list needs when
it is read again once a second.

While a menu stands nothing behind it is reached. That is the application's
part: `ui::menu::hold_input` makes the menu layer the modal layer of the toolkit
and surrenders the keyboard of whatever held it, and `App::active_contexts`
answers `CONTEXT_PALETTE`, in which nothing is bound. The widget takes the keys
it walks by out of the input, so a view that reads the input itself does not
walk along.

The menu always stands in the middle of the window and never follows the
pointer. It closes on `Esc`, on a choice and on a click beside it — except the
question about a lost source, which uses `Beside::Ignored`.

## Sources

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
| both kinds: last connection, transfer directory, profile, offered profiles, variables (`SourceMemory`) | in the console file, or `ports/<key>.yaml` |
| a device: line parameters, offered speeds (`PortMemory`) | `ports/<key>.yaml` |
| commands the shell marked | `history/<key>.yaml` |
| values typed into the ask window | `answers/<key>.yaml` |

Nothing is shared: the parameters of one device never reach another, and
`Settings.line` is only what a device that was never used starts with. A console
file carries nothing of a line, and nothing of a line can reach one —
`App::set_line_params` asks the session which *device* it is on.

Three empty values mean something: `profiles` empty is every profile,
`baud_rates` empty is the shared list, `variables` empty is a source that
answers no name. `variables` is a list, not a map, because it is edited a letter
at a time.

`ConfigStore::save` encodes, compares with the file and returns without writing
when they match. A write that changes nothing still costs a rename and a new
mtime.

### Consoles

A console is its `ConsoleId`. The name is what it is called and may change at
any time, so the file, what is remembered, the answers, the history, the default
source and `--console` all point at the identity.

`consoles::shipped` is what the program comes with: the shell of this machine
and `SSH`, which names `{remote_user}` and `{remote_host}` rather than carrying
them, runs with `-e none` so `ssh` keeps no escape character out of the stream
and `-o NumberOfPasswordPrompts=1` so a wrong password ends the console instead
of asking twice more, and is not trusted. `add_shipped` writes at startup the ones the directory has
never seen, recognised by identity — so a renamed one is not written again, but
a deleted one comes back.

`Console.program` is one line read with `shlex`: `ssh -p 2222 host` is a program
and three words. Nothing is run through a shell, so an unclosed quote is an
error rather than a program with a strange name.

`Console.directory` becomes the working directory of the pty. Empty, the console
starts where the application stands.

`Console.restart` reopens the console when it ends with code 0 — a log follower,
a board that reboots. Anything else asks instead, because a program that cannot
start fails again at once and a window that keeps restarting it does nothing
else.

### Ending

```
exit code 0, no restart  →  the window has no source, shows the last screen
exit code 0, restart     →  the same console is opened again
anything else            →  App.lost: connect again / choose another / close
cannot be opened at all  →  the same question
```

`App.lost` holds what is asked about and why; `App::ask_about_lost` opens the
menu with the failure as its notice. `LostReason::Ended` carries the exit code,
and no code at all — a program that closed its handles and stayed — is not an
ordinary end either. `Esc` is answered as *choose another*, because a question
waved away must not be the thing that closes the window. The failure is printed
in the terminal too.

### The serial driver

`zyt-serial` talks to the platform directly on Linux and through the
`serialport` crate on Windows. The crate is declared for the Windows target
only.

| | linux | windows |
| --- | --- | --- |
| open, line parameters | `open(2)`, `TCSETS2` | `serialport` |
| claim on the node | `TIOCEXCL` and `flock(2)` | `serialport` |
| read timeout | `poll(2)` on a non-blocking descriptor | driver timeout |
| modem lines | `TIOCMGET`, all six at once | one call per line, four of them |
| bytes still on the line | `TIOCOUTQ` | `ClearCommError` |
| enumeration | `/sys/class/tty` | `serialport` |

`TCSETS2` carries the speed as a number, so a rate no constant names is asked
for like any other. A `poll` per read is what keeps the worker answering its
commands and the modem lines while the line is silent.

Two claims are laid on Linux because two kinds of program answer them:
`TIOCEXCL` stops every later `open` of the node, and the advisory lock is the
one other terminal programs take. A port held elsewhere is `PortError::Busy`.

### Enumeration

On Linux a scan walks `/sys/class/tty`: a tty backed by hardware carries a
`device` entry and a virtual console does not, which is the whole filter. USB
descriptors are read from the parent devices. No udev database is asked for,
because there is none inside a container. Each entry carries `accessible` from
`access(2)`; the source menu lists only what the process may open and counts the
rest.

A scan reads the file system, so it runs only while the source menu stands, once
a second (`App::list_sources_again`, `PlateMenu::refill`).

### Windows

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
the pointer, and `App::handle_keyboard` reads no key at all, the same as behind
a menu. `Enter` walks the fields and lands on the button that connects; the
cross and `Esc` both lead back to the source list with nothing connected.

Answers are kept in `answers/<key>.yaml`. They are not `SourceMemory.variables`:
a value there is a decision about the console, an answer here is what it was
pointed at this evening. `answers::fill` puts them into a *clone* of the console
being opened, only where its own memory is empty, on every path into a console.

`Console.name` carries values too, so what a console is addressed by and what it
is shown as differ: `Console::key` is the name as written,
`Console::display_name` has the values put in, and `Session::console_shown`
keeps what was shown from the moment it started. Nothing is quoted there — a
console is not run through a shell — and a name with no value is left standing.

## Command history

Shell marks (OSC 133) say where a command begins and where its output does. The
line itself is in neither, so the emulation reads it out of the grid: the cursor
point is recorded at `133;B` and `Term::bounds_to_string` takes the text from
there to the cell before the cursor at `133;C`. It leaves the crate as
`TerminalEvent::Command`. Recording at `133;C` means the exit code plays no
part — a command that failed is exactly the one somebody wants back.

`App::write_down_commands` drains them each frame into `history/<key>.yaml`.
Three rules:

1. **Read again on every look.** Several copies may share a console, so nothing
   is cached — not for the menu, not for a write. A chosen entry is looked up in
   a freshly read list.
2. **Changed under a lock.** `ConfigStore::save` is atomic within one process
   only, so `history::remember` takes a blocking `File::lock` on
   `history/<key>.lock` for the whole read-change-write.
3. **Kept once.** An equal entry is removed before the new one goes to the
   front, carrying the directory and the moment of this run.
   `Settings.command_history` caps the count.

`ctrl+shift+r` opens the list. A plate is one line: line breaks become spaces,
longer than 64 characters is cut with an ellipsis, and the whole command, its
directory and its last run stand on the plate beside the menu. The button is
drawn only while the file has something in it, asked by file size rather than by
parsing, because it is asked every frame.

`Settings.command_history_keys` says what each way of choosing does — `Enter`
runs the command, `Shift+Enter` leaves it standing, unless turned over. Running
means a carriage return, the way every line this program types into a device is
closed. Which key chose it arrives as `Chosen.held`.

Nothing here is ever reported to the user: a history is a convenience, so a file
that cannot be read, written or locked leaves a log line and an empty list.

## File transfer

`TransferProfile.pty` says whether the program holds the line.

```
pty = true   the program is the far end of a conversation on the device console
             local line + remote line + delay each + finish key, one at a time
pty = false  the program runs beside the line: stdin closed, output to a .txt,
             any number at once, nothing reaches the device
```

Placeholders in a command line, each substituted value quoted for the shell:

| form | meaning |
| --- | --- |
| `{name}` | a value of the source |
| `{>file}`, `{>directory}`, `{>files}`, `{>directories}` | what the transfer carries |
| `{:filename}`, `{:stem}`, `{:suffix}` | parts of its name |
| `{}` | belongs to the program on the line |

Anything else in braces is left standing.

```
start ─► remote line (delay) ─► local line (delay) ─► program ends
      ─► wait for pending_output == 0 ─► finish key ─► transfer ends
```

Both delays are counted from the start, which is how a profile decides the
order: `Cat file` sends `cat > {:filename}` to the device at once and starts
`cat {>file}` here 700 ms later. Both lines and the result are printed into the
terminal.

The program ending is not the transfer ending: on a slow line the driver still
holds bytes. `pending_output` is the outgoing buffer plus `bytes_to_write` of
the driver. While it drains, the bar shows `TX <n>` and the transfer can still
be cancelled. Then the profile may send one key, such as `ctrl+c`.

A transfer holds the keyboard while it runs, so a run that never ends would hold
it for good. The exit code is sent before the flag that says the program ended,
the channel is drained once more after it, a program gone without a code is
reported as stopped, and a local program that fails to start takes its transfer
with it.

A second transfer on the line is refused (`AppError::TransferRunning`).
Disconnecting stops a transfer first and waits for it. The application keeps the
port: `zyt-xfer` starts the local program through `sh -c` or `cmd /C` with pipes
on all three channels, and the application pumps device → stdin, stdout → device,
stderr → terminal notices. While it runs the device bytes are not fed to the
emulator, so the protocol sees a clean stream.

A program on the device that reports progress (OSC 9;4) gets a bar left of the
status bar buttons for as long as it reports — any program may say it, with no
transfer anywhere. The report is dropped when a transfer starts and ends.
`taskbar::show` hands the same share to the Windows shell (`ITaskbarList3`,
created once on the ui thread). Wayland has no such protocol, and the D-Bus
interface the Linux desktops read names the application rather than the window,
so nothing is sent there.

### The panel of what runs

A transfer holding the line, transfers beside it and file tasks are one list: a
fixed window with a title, a cross, and a row each — what it does (cut to 64
characters, whole on the pointer), how long it has run, the button that stops
it, and, for a transfer beside the line, the buttons that drop the row with its
output file and open that file. Row colour is the outcome: green done, error
colour failed or cancelled, none while running.

There is no button that stops everything. The status bar carries one button for
the panel: hovering shows it read-only, pressing leaves it standing with its
buttons. The way to stop the transfer that holds the line is over the terminal,
in its upper right corner, because while it runs the window is doing one thing.

## The plate of the times

Hovering the connection label shows it; the right button pins it
(`data_plate_pinned`, with `data_plate_hidden` so it really goes, since the
pointer that pressed is resting on it). Pressing the plate takes it down. The
left button is already the session menu.

Three rows, in the order they happened, each with a clock reading and how long
ago: input stopped (`Session::last_written`), data began (`first_data`, cleared
by every write), data ended (`last_data`). Data that began and ended at one
moment says how long ago once. `session::Moment` keeps both a timestamp and a
monotonic reading, because a clock put right mid-session would make ten seconds
ago come out as an hour.

Above the rows stands the track, `statusbar::delta_track`:

```
    [span]        [span]            [span]
  *───────────*─────────────*──────────────────────►
  end of      data          data              now
  input       began         ended
```

- `track_marks` builds one mark per *place*. Moments that fall together share a
  dot and stack their names; data that began and ended at one moment is named by
  its end alone.
- The stretch leaving a mark is the wait before the answer (warning colour) or
  the data itself (selection colour), decided by `Mark.began`.
- It is not a scale: every finished stretch takes a quarter of the track
  (`STRETCH_SHARE`) and the silence takes the rest. A 50 ms wait beside a five
  minute silence would otherwise be a six-thousandth of the track. The true span
  is the number written above each stretch.
- A label that would collide with its neighbour is dropped; two names run
  together say less than one.

## Key bindings

`zyt-keymux` takes `KeyStroke` values, walks the active context stack and
answers `Command`, `Pending` or `Unhandled`. The application maps egui events to
strokes, runs the commands and sends the rest to the terminal as bytes. The
palette searches the registry with `nucleo-matcher`; titles are translated
before registration.

While the terminal has the focus the widget holds the toolkit focus and locks
Tab, the arrows and Escape, so only bindings can take a key from the device.
`ctrl+shift+tab` hands the keyboard to the status bar.

Keys are not dispatched at all while a menu, a confirmation or the ask window
stands.

`ctrl+shift+o` (`port.choose`) shows the source menu and nothing else: choosing
is what ends a connection, since `connect_serial` and `connect_console`
disconnect before they open. `port.disconnect` is a menu entry with no key,
because it cannot be taken back.

## What a program can ask for

| sequence | answered by |
| --- | --- |
| title (OSC 0, 2) | the window title |
| clipboard (OSC 52) | the setting below |
| hyperlinks (OSC 8) | a link per cell |
| working directory (OSC 7) | the directory of the process |
| notifications (OSC 9, 777) | `notify-rust`; the two forms are chosen apart |
| shell marks (OSC 133) | the command history |
| progress (OSC 9;4) | the bar of the status bar, and the taskbar |
| colours (OSC 4, 10, 11, 12, 104, 110–112) | the palette of the terminal |

`Settings.osc` decides which are honoured, one choice per sequence and per kind
of session: a column for trusted output, a column for untrusted. `App::osc`
picks the column, so connecting elsewhere changes what is allowed.

Trust is `Console.trusted`, a judgement about a console and not a fact about the
machine: a console carrying somebody else's output — an `ssh`, a log — is not
trusted wherever it runs, and a device never is. The sign in the status bar is
also the switch (`App::toggle_session_trusted`); on a line it cannot be pressed.
Untrusted starts at copy-only clipboard, with notifications, title, links and
colours refused, keeping only its shell marks.

A colour a program set travels in the snapshot and not in the theme:
`RenderableContent` carries `background`, `foreground`, `cursor_color` and
`palette` — the entries of the 256 colour table it painted over, empty until one
does. `TerminalView` prefers them over the theme, so the switch is one `if` at
the point of drawing and refusing it needs nothing repainted or reparsed.

Each choice is enforced where the sequence is acted on, never in the parser:
the clipboard in `zyt_term::Terminal`, title and notifications in
`App::handle_terminal_events`, links by `TerminalView::links`, colours by
`TerminalView::program_colors`, marks by `Session::marks_enabled`. What is
refused is dropped, not buffered, and a refusal is silent.

Clipboard settings:

| setting | storing | reading |
| --- | --- | --- |
| disabled | no | no |
| copy only | desktop clipboard | no |
| copy and limited paste | desktop clipboard + `App.stored_clipboard` | from `stored_clipboard` |
| copy and paste | desktop clipboard | the desktop clipboard |

Limited paste answers without asking the toolkit, so a program gets back what a
program of this window stored and what the user copied elsewhere stays where it
is. To the terminal it looks like `ClipboardAccess::CopyPaste`, because the
difference is who answers, not whether reading is honoured.

The clipboard belongs to the toolkit, so paste and an honoured request both use
`ViewportCommand::RequestPaste` and answer when the paste event arrives.

`App::open_link` refuses a `file://` path while the session is untrusted, and
the terminal says why.

Every icon is a code point in `src/ui/icons.rs`, and a test there asks the
toolkit whether its fonts carry each one: only two of the fonts egui ships with
answer the proportional family, so a plausible code point is as likely to be
drawn as a box. The test asks the charmaps of the family, not `Fonts::has_glyph`,
which answers no for a code point whose first face is the one the replacement
glyph comes from. `experiments/glyphs` lists what the fonts do carry.

## The bell

`BEL` asks for the person, so it asks the desktop:
`ViewportCommand::RequestUserAttention` when the window is not focused, and
nothing at all when it is. The flag is taken down here on the first focused
frame — the toolkit says it resets on focus, but that belongs to the window
manager — and `App.attention_asked` keeps it from being sent every frame. The
`BEL` that terminates an OSC sequence is consumed by the parser.

## What can be done with a file

A right click asks the terminal what is under the pointer, every time:

| under the pointer | the menu |
| --- | --- |
| a selection | terminal menu, copying offered |
| nothing selected | terminal menu, copying out of reach and saying why |
| a `file://` path, trusted | the menu of that file or directory |
| a `file://` path, untrusted | copy address, copy text, terminal entries |
| any other link | open, open with, copy, terminal entries |

The first plate names what the menu is about and cannot be chosen. The link
comes from the click that opened the menu and nothing else. A program grabbing
the mouse gets the click instead.

With a selection standing, the three entries that write into the terminal are
left out: the menu was opened to act on the selection.

File operations run on threads of their own and stop where they are: a copy
moves in chunks and removes what it half wrote; a deletion removes one entry at
a time, keeps what it already removed, and never follows a link out of the tree.
The trash is one call to the desktop, so cancelling is answered before it starts.

The clipboard carries text and pictures, so a file goes to it as the type
`mime_guess` reads from the name; anything else is named in a notice.
`Settings.file_menu.copy_limit` caps it — the clipboard hands the whole file to
every asker and a manager keeps a copy. `Settings.file_menu` and
`directory_menu` decide which entries exist and which removals ask first: the
trash does not ask, deleting for good does.

A dropped file (`App::handle_dropped_files`) opens the menu of that file
whatever the session is: send it, insert its path quoted, insert its contents
when they are text small enough, or pick another profile first — which reopens
the same menu, so the file survives the choice. Only X11, Windows and macOS
deliver drops; Wayland reports none.

Confirmations are one modal (`src/ui/confirm.rs`) answering `Yes`, `No` or
`Pending`, with what is waiting in `UiState::pending_delete`.

## Fonts

Nothing is installed and nothing is copied: a font is mapped, not read.
`FontData::from_static` over a mapping is held by reference (`Blob` is
`Arc<dyn AsRef<[u8]>>`), so pages are read as they are touched. Measured on a
0.7 MiB font: 2.4 MiB held, 1.4 MiB mapped. `fonts::mapped` maps each file once
for the life of the process.

```
terminal  FontFamily::Name("terminal")  chain of families, first is the grid
interface Proportional + Monospace      one family
          ↓ behind both
          the fonts egui ships with, then fonts::add_fallbacks (locale scripts)
```

The terminal takes a chain because one family rarely carries everything a device
prints; every link must be monospaced, wherever it stands. The interface is one
family and may be any of them: it is prose. The terminal family is always bound,
with the toolkit's monospaced fonts behind or instead of the choice, because a
family nothing is bound to panics the first time a glyph is asked for.

`fontdb` reads the families, and only where somebody asked for the list:
`App::font_families` reads them when one of the two menus is opened, never for
drawing. Walking the font directories is the longest wait a window has. So a row
of the chain is just a name; whether the machine still has that family, and
whether it is monospaced, are answered by the lists that ask — the add list
offers monospaced families only, the interface list marks a chosen family as
gone.

Monospacing is read from face metadata, not by loading files. A name that names
nothing is logged and skipped, and the settings still show it.

Fonts are replaced between passes, never inside one (`App::apply_fonts` runs
before the frame). The first window is settled in `App::new`.

`Settings.font_size` and `interface_font_size` are points. The interface size is
the body text; `App::apply_interface_size` derives the other styles from the
toolkit's own ratios, always from the shipped sizes and never from the current
ones, which would grow a little every frame.

## Themes

`TerminalTheme` is 22 colours. `Built-in dark` and `Built-in light` are compiled
in; the rest are files in `<config>/themes` in the Alacritty colour format.
`ThemeCatalog` reads the directory at startup and on demand, keeps the built-ins
first and names files it could not read instead of failing. What a file leaves
out keeps the value of the built-in of its mode, and the background decides that
mode.

`Settings.themes` names one palette per colour mode;
`App::apply_terminal_theme` resolves it when the mode or the setting changes,
falling back to the built-in when the name resolves to nothing.

The palette is the terminal's alone — the interface takes `egui::Visuals`. A
program may set the terminal background and foreground (OSC 10, 11); every panel
takes the same background, so a program that paints itself dark leaves no band
of the theme beside it, and the scrollbar lies over the text. A cell is painted
in its own rectangle and nowhere else, so a row that sets a background stops
where its last cell does.

`dark-light` reports the desktop setting and is subscribed to — one D-Bus signal
of the portal on Linux — falling back to a once-a-minute read where the platform
cannot notify. Nothing queries D-Bus per frame.

## The settings page

Two halves (`SettingsTab`), because the questions are two.

| *General* | *Connection* |
| --- | --- |
| appearance, fonts, performance | the console's name, program, directory |
| what a program may ask for | or a port's offered speeds |
| the library of transfer profiles | the values this source answers |
| | which profiles it offers and uses |

The *Connection* picker holds the consoles and the port this window is on. A
console is a file somebody meant; a port is a device the system found, so the
one worth a page is the one on the line. The page follows the connection until
something else is picked, and a pick that names nothing falls back to that.

Name, program and directory are text with a pencil to the left, not open fields:
a page of fields looks like a form whether or not anything is being changed. The
pencil is drawn apart from the value, so the directory can put its picker button
between them. The keyboard leaving closes the editor; the file is written when
the writing ends and the value changed, never per letter. `UiState.editing`
holds which one is open and what it held when it opened. The text shown is the
line with `{name}` resolved; what is written is the line itself.

Under the values stands a row per asker — the console, each offered profile —
with the names it wants. A name with a row is struck through; a name without one
is the button that makes it. A profile asking for something the source has not
got carries a warning triangle and is *not* switched off: whether it is offered
is the user's answer. It is left out of the menu that starts one, because that
menu is opened to start something now.

Ticking every profile is written down as ticking none, so a profile shipped
later is offered rather than quietly left out. Transfer profiles are a list the
user edits as a list, so they live in `profiles.yaml` of their own, and shipped
ones are added the way shipped consoles are.

Two sections fold under the sequence they belong to: the `file://` menus under
OSC 8, and the size of the command history under OSC 133.

## The window of numbers

`Settings.show_debug_window` opens it: processor share, memory now and peak,
threads with their names, frames, scrollback fill and cost, the buffers
(clipboard, pending output, read buffer, bytes in and out, last busy period),
fonts and atlas. Only the cross can be pressed. It scrolls past seven tenths of
the window height.

`src/metrics.rs` reads `/proc/self` twice a second, not per frame: a number that
moves every frame cannot be read, and the processor share needs two readings to
exist. Other platforms report `None` and the window shows a dash.

The *Render* block holds the last frame time — the work of one pass, without
what the toolkit spends afterwards — and the frames counted in the last second.
A rate derived from one frame time is a rate the program never drew at.

## Errors

Library crates return closed enums with `#[source]` causes and English
diagnostic messages. The binary maps each variant to a translation key in
`src/error.rs`, so all localized text is in `locales/app.yml`.

The message is printed into the terminal in a box drawn with frame characters,
with the chain of causes; the frame carries the colour and the text keeps the
normal foreground. Repeated identical blocks are suppressed. There is no message
area: a failure stays next to the output it belongs to.

## Where the application stands

The working directory of the process is the only answer, and `enter_directory`
in `src/app.rs` is the only thing that moves it. A console profile moves it at
the connection and OSC 7 moves it when the shell says so. The file dialog, a
dropped file, a file move and a new window all read it. `zyt-pty` hands
`portable-pty` that directory — it would use the home directory otherwise — and
knows of no other.
