# Architecture

This file is the map: the crates, the threads, the path the bytes take, and
where each subject is written down. The chapters are in `docs/`.

| chapter | subject |
| --- | --- |
| `docs/data-path.md` | read buffer, the ladder of waits, frames and idle cost |
| `docs/ports.md` | port state machine, what this side drives on a line |
| `docs/terminal.md` | emulation, rendering, fonts, themes |
| `docs/sources.md` | ports and consoles, what is remembered, command history |
| `docs/ui.md` | main area, selection, the menu, settings, the window of numbers |
| `docs/signals.md` | the plate of the times and the plate of the signals |
| `docs/transfer.md` | file transfer, the panel of what runs, files |
| `docs/scripts.md` | transfer scripts: the engine, where they are found, how one is stopped |
| `docs/osc.md` | what a program can ask for, trust, the bell |
| `docs/desktop.md` | the icon of the executable |

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
├── zyt-script    mlua + std::process
├── zyt-files     trash + mime_guess
└── zyt-config    directories + serde_yaml_ng
```

No library crate depends on another, except `zyt-term-egui`, which renders
`zyt-term`. Only the binary knows all of them and only the binary holds user
facing text. What a caller has to know about a crate is in its own README.

## Threads

```
ui thread        draws, reads settings, owns Session and Terminal
port worker      one per open port: opens, reads, writes, polls modem lines
pty read/write   one pair per console
script           one per running script, plus one per program it started
transfer         one per file task
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
and one pointer swap per frame, and no allocation once both buffers stand at
their size. `zyt-pty` and `zyt-script` use the same type.

Control information — connection state, modem lines, errors, transfer progress —
travels as messages (`PortEvent`, `TransferEvent`): it is small and rare.

The size of the buffer and how long the bytes wait in it are in
`docs/data-path.md`.

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

What the desktop does with that name is `docs/desktop.md`.

A window whose `app_id` names no desktop entry cannot be matched to the program:
the desktop draws it with the default icon and lists it a second time. The
window title (`main::WINDOW_TITLE`) is read by a person and changes with the
session.

The configuration directory is resolved from the same identity through
`directories`, which asks for the application name alone on Linux and for the
organization as well on Windows.

## The plate of a block and the command history

These two subjects live in separate chapters — `docs/ui.md` and
`docs/sources.md` — and they do not change separately. The plate of a command of
several lines (`src/ui/block.rs`) is the one place in the program where a person
writes a command out and sends it, so what it sends is also one entry of
`history::List::Source` for that source. What is written down is the text of the
field, not the bytes that went out.

| the plate holds | the entry holds |
| --- | --- |
| the text of the field, carets and all | `Entry::command`, that same text |
| the switch that reads the carets (`UiState::block_caret`, kept in `SourceMemory::block_caret`) | `Entry::caret`, what the switch was at that moment |
| the directory the session stands in | `Entry::directory` |

A change to the plate — what it sends, what it keeps, a switch added to its row
— is read against four places before it is made:

- `history::Entry` and `history::remember`, which is what has to carry the new
  thing. A switch the entry does not carry is a command whose plate lies about
  how it ran.
- `ui::history::whole`, the plate beside the menu, which is where a person finds
  out what an entry was sent as.
- `App::run_from_history`, which types the text back the way `Entry::caret`
  says it ran: the carets of such an entry are read a second time
  (`typed_again`) and the breaks of it are not, and nothing closes it
  (`closes_a_command`) — the notation already says where the command ends. For
  every other entry the carriage return is the key it was chosen by.
- the files already written. A new field of `Entry` is `#[serde(default)]`, by
  `RULE config.legacy`: an entry written before it reads as the default of it.

One rule holds on both sides of this: with the carets read, nothing of this
program closes the command. `typed_block` adds no return to a block whose text
does not carry one, and `closes_a_command` answers no whichever key the entry
was chosen by. A control code of such a command is one somebody wrote, and that
includes the `^M` that runs it.

Two things the rule of one entry per command settles. An entry is replaced by `command` alone, so
two runs of one text with the switch turned differently are one entry and the
last run is what it says. And `Settings.command_history` of nothing keeps no
history at all — the plate still sends, so nothing of the sending may depend on
the entry being there afterwards.

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
