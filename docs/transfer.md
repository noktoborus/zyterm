# Transfer and files

The profiles, the placeholders, the finish key and what runs beside the line are
`crates/zyt-xfer/README.md`; the file tasks and what each one guarantees are
`crates/zyt-files/README.md`. This chapter is how the window runs them.

## A transfer that holds the line

`TransferProfile.hold_line` says whether the program holds the line. One such
transfer runs at a time, a second is refused (`AppError::TransferRunning`), and
it owns the keyboard while it runs.

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

A run that never ends would hold the keyboard for good, so the end is reported
in a fixed order: the exit code is sent before the flag that says the program
ended, the channel is drained once more after it, a program gone without a code
is reported as stopped, and a local program that fails to start takes its
transfer with it.

The application keeps the port while a transfer runs and pumps it: device →
stdin, stdout → device, stderr → terminal notices. The device bytes are not fed
to the emulator meanwhile, so the protocol sees a clean stream. Disconnecting
stops the transfer first and waits for it.

## Progress a program reports

A program on the device that reports progress (OSC 9;4) gets a bar left of the
status bar buttons for as long as it reports — any program may say it, with no
transfer anywhere. The report is dropped when a transfer starts and ends.
`taskbar::show` hands the same share to the Windows shell (`ITaskbarList3`,
created once on the ui thread). Wayland has no such protocol, and the D-Bus
interface the Linux desktops read names the application rather than the window,
so nothing is sent there.

## The panel of what runs

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
the mouse gets the click instead. With a selection standing, the three entries
that write into the terminal are left out: the menu was opened to act on the
selection.

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
