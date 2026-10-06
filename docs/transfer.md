# Transfer and files

A transfer is a script: what a script may call is
`crates/zyt-script/README.md`, how one is found and run is `docs/scripts.md`,
and the scripts that ship are `scripts/README.md`. The file tasks and what each
one guarantees are `crates/zyt-files/README.md`. This chapter is how the window
runs them.

## A script that holds the line

`Manifest.hold_line` says whether the script takes the line. One such run at a
time, a second is refused (`AppError::TransferRunning`), and it owns the
keyboard while it runs.

```
start ─► the script runs ─► the script ends
      ─► wait for pending_output == 0 ─► finish key ─► the run ends
```

What the script does in between is its own business, and that is the whole of
the change: the two delays of a profile and the order they decided are now a
`zyt.time.sleep` where the script wants one. `docs/scripts.md` carries what
happens to the line, the keyboard and the terminal while it runs, and the
ladder a run is stopped with.

The script ending is not the run ending: on a slow line the driver still holds
bytes. `pending_output` is the outgoing buffer plus `bytes_to_write` of the
driver. While it drains, the bar shows `TX <n>` and the run can still be
cancelled. Then the manifest may send one key, such as `ctrl+c`.

The application keeps the port while a script runs and pumps it: device → the
script, what the script wrote → device. The device bytes are not fed to the
emulator meanwhile, so a protocol sees a clean stream, and what the script has
to show it writes into the terminal itself. Disconnecting stops the script
first and waits for it, bounded: a script that answers nothing is given up on
and its line is shut under it.

## Progress a program reports

A program on the device that reports progress (OSC 9;4) gets a bar left of the
status bar buttons for as long as it reports — any program may say it, with no
transfer anywhere. A script says it with `zyt.progress`, and the two write the
same place. The report is dropped when a run starts and ends.
`taskbar::show` hands the same share to the Windows shell (`ITaskbarList3`,
created once on the ui thread). Wayland has no such protocol, and the D-Bus
interface the Linux desktops read names the application rather than the window,
so nothing is sent there.

## The panel of what runs

A script holding the line, the programs it started beside the line and file
tasks are one list: a fixed window with a title, a cross, and a row each — what
it does (cut to 64 characters, whole on the pointer), how long it has run, the
button that stops it, and, for a program beside the line, the buttons that drop
the row with its output file and open that file. Row colour is the outcome:
green done, error colour failed or cancelled, none while running.

There is no button that stops everything. The status bar carries one button for
the panel: hovering shows it read-only, pressing leaves it standing with its
buttons. The way to stop the script that holds the line is over the terminal,
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
when they are text small enough, or pick another script first — which reopens
the same menu, so the file survives the choice. Only X11, Windows and macOS
deliver drops; Wayland reports none.

Confirmations are one modal (`src/ui/confirm.rs`) answering `Yes`, `No` or
`Pending`.
