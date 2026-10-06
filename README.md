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
| `ctrl+shift+r` | the commands the shell marked, and the ones added by hand |
| `ctrl+shift+enter` | a plate for a command of several lines; `ctrl+enter` or its button sends it, `esc` drops it |
| `ctrl+shift+c` / `v` | copy, paste |
| `ctrl+shift+arrows` | pick out a block from where the cursor stands; let the keys go and it stays. Held against the left or the right edge it goes on widening from the other side |
| `ctrl+shift+home` / `end` | take the block to the start or the end of the row its edge stands on |
| `esc` | leave the selection; that press alone does not reach the device |
| `Copy`, `Cut`, `Paste` | a keyboard that has keys of its own for them copies and pastes with them; nothing is sent to the device |
| `ctrl+shift` held | take the mouse from a program reading it, or hand it back |

`ctrl+f` stays free, so `^F` still reaches the device, and `ctrl+c` is `^C` as
it has to be — copying is the one with `shift`. Bindings, contexts and sequences
are YAML in the config directory, and every command is in the palette.

## Sources

Serial ports and local consoles are one list: a menu of plates showing the path
or the program, the name of the device, and when it was last opened. Typing
searches it by either. It is ordered by last use, read again once a second while
it stands, and ports the process cannot open are counted but not listed.

A device is followed by USB identity, not by path, so one that comes back as
`/dev/ttyUSB1` is the same device. Unplugging releases the handle at once.

### A console

| | |
| --- | --- |
| the command line | split the way a shell splits it and run without one, so `ssh -p 2222 host` is a program and three words |
| where it is kept | a file under `consoles/`, named by an id renaming does not touch |
| the directory | it may name the one it starts in |
| starting again | it may be told to, when its program ends with code 0 |
| a value it has no answer for | asked before it opens — `ssh {remote_host}` — and kept per source, so the same board is `Enter` and another one is the difference typed over it |
| any other end, or a source that cannot be opened at all | connect again, choose another, or close; the output stays on the screen while it asks |

### The controls of a line

| | the left button | the right button |
| --- | --- | --- |
| `BRK` | holds the line in the break condition | — |
| `HOLD` | stops the source being read | — |
| `RTS`, `DTR` | holds the line, or hands it back to the driver | which way a hold takes it, down or up: one list per line, opened on the direction that line is on |
| the plate of the signals | the pointer resting on it raises the plate; a press leaves it standing | the same |

The letters stand pressed while the line is driven from here, and their colour
is what the line is doing. The palette carries each as a command — *toggle
BREAK*, *toggle HOLD*, *toggle the plate of the signals* — because a line
watched while both hands are typing cannot be a line watched by holding a
pointer still.

A break is a state and not a key: it lasts until it is let go, which is what a
device reading a break as a request for attention waits for. It leads the row
because a held break is a line carrying no byte at all, and that is what
explains every reading beside it going quiet.

Which level holds a board in reset is a fact about that board, so the direction
is said once and written down for the device, and a press is then a press; a
line already held is held the other way as soon as the other direction is
picked. A forced level and a held break are put back on the line every time the
port opens, so a board held in reset stays in reset across a replug, across a
disconnect and across a restart of this program. Left to the driver, nothing is
written to the line at all.

`HOLD` asks for the state the read buffer reaches by itself when a window cannot
keep up. Where the bytes gather instead is what the source is, and the hint says
which of the three:

| source | where they gather | what the device is told |
| --- | --- | --- |
| a port with flow control | the driver | to wait: `RTS` on the plate falls for exactly as long |
| a port without it | what the driver cannot hold is lost | nothing |
| a console | the pipe of its pseudo terminal | nothing; its program waits at its next write |

A console has no lines, so its own `HOLD` stands beside the button of the
statistics and nothing else.

### The plate of the signals

One button at the head of the line controls raises it over the terminal. It is a
button of its own because every letter beside it is worked, and a plate that
rose from them would rise every time one was pressed.

| row | what it shows |
| --- | --- |
| one per line | filled where the signal stood and empty where it did not, what this side drives above what the device does |
| one per direction of the data | the same, so a line that went quiet is read against the handshake beside it |
| the two queues of the driver | as tall a share of their row as the queue is of the fullest it has been seen, which is the only way to know how much fits, since nothing asks the driver that |

It answers the one question a serial line always raises — did it go quiet by
itself, or did a signal stop it — which a row of letters saying what is true
*now* never could. It stands against the edge the terminal cursor is furthest
from, so the rows being written into are the rows it never covers. One bar is
one reading of the lines, so how far back the plate reaches follows how often
they are read, and the span is written under it.

It stands for a console as well, and a console shows what a console has: how
much it said and when, how much was typed into it, and the hold that stands
while what it said has not been taken.

Which letters a device shows is its own setting, and the plate draws the same
set: the row and the tracks are the same signals read two ways. Six of the eight
stand to begin with — a `DCD` tied high by an adapter and an `RI` wired to
nothing are noise in a row read at a glance and flat tracks in a picture read
for the one that is not flat, so a device that uses them says so. `RTS` and
`DTR` say what their two buttons do and which way the line is held.

### What one port is asked

| setting | what it answers |
| --- | --- |
| empty the driver buffers as the port opens | a session does not begin in the middle of a sentence nobody asked for |
| `HUP` | whether the driver drops `DTR` and `RTS` as the port closes, which is how a device is told the session ended: a modem hangs up, a board wired to reset restarts. A `termios` flag, offered only where the platform has one; on Windows the lines on close are the driver's business and the setting is not drawn at all |
| how often the modem lines are read | every reading is a call into the driver on the thread that reads the port: a line watched closely is a line read less |

### Giving up on what is going out

`TX <n>` on the right of the bar is how many bytes have not left yet, and it is
a button: pressing it gives up on them. That is the way out of a paste nobody
meant to make on a line too slow to carry it, and the palette carries it as
*clear the queue of what is going to the line*. What reached the line is gone;
everything still in a buffer is emptied —

- what was pushed for the port,
- what the driver would not take,
- the queue of the driver,
- the answers the terminal owes a program,
- and a running transfer, which is stopped because otherwise it would fill the
  queue again on the next frame.

## Terminal

The operating system commands the terminal answers. The *OSC support* settings
decide which of them are honoured; a sequence they refuse is dropped without an
answer, so a program asking for more than it may have is left waiting for
nothing.

| sequence | what it says | how it is answered |
| --- | --- | --- |
| `OSC-0`, `OSC-2` | the window title | The window wears it while *Window title* is honoured. A program that resets it gives the window the name of the application back. |
| `OSC-4`, `OSC-10`, `OSC-11`, `OSC-12` | the colours a program paints itself in | `4` paints over one entry of the 256 colour table, `10` and `11` move the default pair, `12` names the cursor. All four behind *Palette*, and `104`, `110`, `111` and `112` give a colour back. A colour is read as `rgb:rr/gg/bb` or `#rrggbb`, never as a name. |
| `OSC-7` | the working directory | It becomes the directory of the process, which is where the file dialog and a transfer start, while *Working directory* is honoured. It starts honoured for trusted output and refused for the rest, because it moves this window on the word of the far side. |
| `OSC-8` | a hyperlink | Drawn as a link while *Links* is honoured. A `file://` address opens the menu of that file instead of the browser. |
| `OSC-9` | a notification with a text | Handed to the desktop while *Notification* is honoured. The heading is the name of the application, since the sequence carries none. |
| `OSC-9;4` | how far along a program is | A bar left of the status bar while *Progress* is honoured, and the taskbar on Windows. All five states are read; a share above a hundred is brought back to it. |
| `OSC-52` | the clipboard | Four settings rather than a switch, from refusing everything to copy and paste. Reading the clipboard is answered from what the program stored itself at the setting between them. |
| `OSC-133` | the marks around a prompt and a command | `A`, `B`, `C` and `D` with the exit code. What stands between `B` and `C` is the command, which is what fills the history. Honoured behind *Shell marks*. |
| `OSC-777` | a notification with a heading | Handed to the desktop while *Notification with a heading* is honoured. Only the `notify` form is read. |

`assets/osc` holds a script per sequence, played once from a console, so what a
switch does can be seen.

### The pointer

A program that asks for the mouse gets every button of it, and what is on the
screen under one — a table, a column of numbers, a log with a prefix — is still
text somebody needs, so there are two ways past it.

| the left button, with | what a press does |
| --- | --- |
| nothing held | Begins a selection on the character the button went down on and grows it while the button is down. One press selects cell by cell, two a whole word, three a whole line. |
| `shift` | Grows the selection that stands to the character pressed on. The half of the selection the press lands in is the end that moves, so it adds on either side and cuts back when the press is inside. |
| `ctrl` | Selects a rectangle rather than a run of text. Only a single press asks for one: a rectangle of words is not a thing to select. |
| `ctrl+shift` | Turns the mouse grab the other way for as long as it is held, so a press selects a run of text in a program that is reading the mouse — and reports to that program while it is not. |
| `ctrl+shift+alt` | Selects a rectangle in a program that is reading the mouse. The press is not reported at all, whichever way the grab stands. |

| | what it does |
| --- | --- |
| the right button, held and moved | scrolls the page |
| the right button, released in place | opens the menu |
| the middle button | pastes the platform selection |
| a held finger | selects the word under it and adds what it moves over |

A selection takes the whole of the character at each end: the one the button
went down on and the one the pointer stands on. It begins where the button went
down and not where the drag was noticed a few points later, and it keeps the
first character whichever way it is dragged — the same two cells give the same
text left to right and right to left.

`Shift` and a press work on the selection that stands rather than turning it
over: the mark of where it begins jumps to the end further from the press, and
the selection runs from there to the press. So a press to the left of a
selection made rightwards grows it leftwards and keeps everything it had, and a
press inside it cuts that side back. Which end is further is asked again on
every press, and it adds characters whatever the selection was picked out by.

### The plates

| plate | raised by | what it shows |
| --- | --- | --- |
| the times | the pointer on the connection | when the input stopped, when the answer began and ended, and how much it carried |
| the selection | a selection standing | how many columns, lines and characters it covers |
| the signals | the button at the head of the line controls | a track per line and per direction of the data |
| what the session may ask for | the pointer on the sign of trust | one row per operating system command this session is allowed, named as the settings name it |

The size on the plate of the times is of that answer and not of the session,
because the question it settles is whether the device said as much as it was
supposed to.

The plate of the sign of trust lists what is honoured and leaves out what is
refused: the question a sign of trust raises is what a guest may do. The right
button leaves it standing, since the left one is the switch of trust itself.

Picking out a selection is a mode: while it stands the pointer is the
selection's whatever a program asked for, nothing is typed into the device, the
counts stand on their plate, and the status bar carries a red mark beside the
sign of trust. It is left by `esc`, by a press on that mark, by a press on the
plate, or by a press in the terminal with nothing held — a press with `shift` or
`ctrl` is building a selection rather than letting one go.

Both ends of a selection are marked in the grid while it is being picked out: a
corner laid into the cell it began in and another into the cell it is growing
into, pointing away from each other so the pair brackets what is taken. Which
corner each takes follows the block and not the order the ends were made in.
Outside the mode they are a setting, *Show the ends of a selection*.

The plate of the selection stands in the corner the growing end of the selection
is furthest from — the pointer of a drag, the caret of the keys — so it never
covers what is being taken, and it is laid on the cell grid of the terminal. It
moves only when the selection does. The counts are of the text the selection
would copy, so they agree with what the program at the other end of the
clipboard counts.

### The rest of the terminal

| | |
| --- | --- |
| a command of several lines | a window with a field that grows and the button that sends: a loop, a here-document or a configuration stands whole before any of it is typed into the device. The whole of it goes into the history of that source |
| search | over the screen and the scrollback: four ways of reading the query, case and highlight switches, the current match as the selection |
| scrollback | a memory budget rather than a line count, because a row costs the full width of the window; a change takes hold without a restart |
| palettes | the Alacritty colour format, read from `themes/` of the configuration, one for the dark mode and one for the light; `assets/themes` holds two examples |
| fonts | the ones the system has: the terminal takes a chain of monospaced families, the interface one family of any kind, and what the toolkit ships with stays behind both, so a glyph nothing chosen carries is still drawn |

A run of NUL bytes is drawn as a block — a red frame, the colours of the cell
exchanged, a mark, and `×` with the count where the run was longer than one
byte. A standard terminal drops the byte, so a device that went quiet in the
middle of a word leaves nothing behind anywhere else, and that nothing is what
somebody watching a line came to see. The mark is drawn by the window rather
than taken from a font, the way Firefox draws the character no font carries, so
it is the same on every machine; the count is text, because digits have to read
as part of the line. `assets/terminal/nul.sh` puts every case of it on the
screen, and `assets/terminal/random.sh` fills the page with characters instead —
which is where a box says nothing in the chain of fonts carries one.

## Files

| the menu of | what it offers |
| --- | --- |
| a `file://` link printed by a console | open, open with, rename, move here, copy the address, the text or the contents, to the trash, delete |
| a file dropped onto the window | send it, insert its path, insert what it holds, or pick another script first |
| the context menu | pick a file or a directory from the dialog and type its path in, quoted the way a shell reads it |
| a selection | save it into a file, *save and open as …*, *Add to the command history* |

Every operation runs on a thread of its own and can be stopped, and the settings
decide which entries exist and which ask first. X11, Windows and macOS deliver
drops; Wayland does not.

A menu opened on a selection offers only what is about that selection: the
entries that write into the terminal are gone, and so are the entries of a
transfer, which is about a file of a machine and not about what is on the
screen.

Saving a selection:

```
entry chosen ──► the text is taken ──► dialog ──► a write task ──► the path
                 at once, so a program               │             ──► the clipboard
                 writing meanwhile cannot            │             ──► open with …
                 change what is saved         can be stopped
```

*save and open as …* hands the file to the program the desktop asks you to pick
— a page of a log is read in whatever reads a log best. Whether the path goes
into the clipboard is a setting. Nothing is said in the terminal about a save: a
plate over the output would cover the very thing that was worth saving, and the
path is what somebody does the next thing with — paste it into a command, into a
message, into the dialog of another program.

*Add to the command history* puts the selection among the commands that list
offers. It is one file for every console and every port, because a line worth
keeping was worth keeping wherever it was read and is often to be typed into
another source. Every entry of the list carries *Remove* one step in, which
takes it out of the file it is kept in — the one of this source, or the shared
one.

## File transfer

A transfer is a script in Lua. It is handed the line of the session, a way to
ask you something and a way to start programs of its own; what it does with
them is its own.

| | |
| --- | --- |
| shipped | the shell transfer, the three modems, `cat`, a listing of the device, and `scp` both ways through the directory the device stands in |
| what one asks you | a window of its own: text, a switch, one of several, one picked from a list, any number of a list — described by the script and drawn by the program |
| on the line | holds the keyboard, is stopped from over the terminal, and counts as finished only once the last byte left the port |
| what it may start | programs with pipes it reads and writes itself, or handed straight to the line; all of them stop when it does |
| the panel | what runs and the file operations in one list, with the time each has been running and the button that stops it |

```
script ─┬──► it types what the device has to run     ─┐
        └──► it starts a program here, or carries    ─┤
             the bytes itself                         ▼
                            the line ──► the queue drains ──► done
                                          TX <n>, still cancellable
```

A script is a directory with a `Manifest.yaml` in it: the name you read, what
it needs picked, what it asks the connection for. Starting the program reads
those and nothing else, so a list of what can be done runs none of it.

A script may stand in four places, and the nearest to you wins: `scripts/` of
the configuration directory, which is where one of your own belongs, then your
own data directory, then the directories of the system, then beside the
program, which is what an unpacked archive runs. So a copy of yours stands in
for one shipped with the program. The settings name each, say where it came
from and open its directory.

What a script asks you is kept for that connection, that script and that form,
so the second file is one press. The switch in the window is what stops a form
from coming up, and *Reset form saving* in the settings puts every one of them
back to asking. A form about what the device holds right now — which file to
take off it — is never kept.

`shell-transfer` carries files to a device with nothing installed on it: a
plain shell on the far end and FISH on the wire, base64 or raw with `stty`,
chunks that each say where they belong, and the sums compared on both sides.
`scripts/PROTOCOL.md` is the wire format and `scripts/README.md` is what each
shipped script needs on the device.

`zyt-script` runs one with no window anywhere — against a shell over pipes,
against a pseudo terminal, or against the standard channels — which is how a
script is written and checked:

```sh
zyt-script list
zyt-script run shell-transfer --direction send --target ./image.itb \
    --line pty --command sh --answer remote=/tmp --answer digest=auto
```

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
| `make install` | the *profiled* binaries, the shipped scripts, desktop entry, icons and `osc133-bash.sh` under `/usr/local` |
| `make uninstall` | takes them back out |

`PREFIX` and `DESTDIR` say where. Installing never builds: it takes what
`make pgo` left and refuses when there is none, so the release path is
`make pgo && make install`.

The desktop entry and the icons are installed under the application identity,
`ru.styxheim.zyterm`, which is also the `app_id` the window carries. The two
have to agree or the desktop cannot tell that the window is this program. The
scripts go to `share/zyterm/scripts`, which is the directory of the machine;
yours go to the configuration directory and are found first.

```
make pgo:  instrumented build ──► cargo test, the workload ──► llvm-profdata
                                  (allowed to fail)             │
                                  a red test still measured     ▼
                                  something              the binaries again
```

`llvm-profdata` comes from `rustup component add llvm-tools-preview`. To profile
the program itself rather than the tests:

| command | what it does |
| --- | --- |
| `cargo pgo run -- --bin zyterm` | runs the instrumented window |
| `cargo pgo optimize build -- --bin zyterm` | builds with what it left |
| `cargo pgo clean` | throws the profiles away |

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
| `history/` | commands a shell marked, one file per source, and `added.yaml` for the ones added by hand |
| `answers/` | values typed before a console was opened, one file per source |
| `scripts/` | transfer scripts of your own, a directory each, which stand in for the shipped ones |
| `forms/` | what a script was answered, one file per connection, script and form |
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
| `crates/zyt-script` | transfer scripts in Lua: the engine, the host calls, the runner |
| `crates/zyt-files` | file operations on their own threads |
| `crates/zyt-config` | configuration directories and YAML files |

`experiments/*` are tools, not parts of the program:

| tool | the question it answers |
| --- | --- |
| `experiments/glyphs` | what the fonts of the toolkit carry |
| `experiments/keys` | what a terminal sends for a key |
| `experiments/flood` | what a terminal does as the bytes come faster |

Each crate has its own README. `ARCHITECTURE.md` is the crates, the threads and
the data flow, and the index of `docs/`, a chapter per subject; `AGENTS.md` is
the rules the code follows.

## License

MIT, the whole workspace. `LICENSE` carries the text; every crate declares it
with `license.workspace = true`.
