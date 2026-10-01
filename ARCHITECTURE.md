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
| `docs/osc.md` | what a program can ask for, trust, the bell |

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
facing text. What a caller has to know about a crate is in its own README.

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
and one pointer swap per frame, and no allocation once both buffers stand at
their size. `zyt-pty` and `zyt-xfer` use the same type.

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

A window whose `app_id` names no desktop entry cannot be matched to the program:
the desktop draws it with the default icon and lists it a second time. The
window title (`main::WINDOW_TITLE`) is read by a person and changes with the
session.

The configuration directory is resolved from the same identity through
`directories`, which asks for the application name alone on Linux and for the
organization as well on Windows.

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
