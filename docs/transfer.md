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
      ─► wait for pending_output == 0 ─► the run ends
```

What the script does in between is its own business, and that is the whole of
the change: the two delays of a profile and the order they decided are now a
`zyt.time.sleep` where the script wants one. `docs/scripts.md` carries what
happens to the line, the keyboard and the terminal while it runs, and how a
run is stopped.

The script ending is not the run ending: on a slow line the driver still holds
bytes. `pending_output` is the outgoing buffer plus `bytes_to_write` of the
driver. While it drains, the bar shows `TX <n>` and the run can still be
cancelled. A key the device is to be left with — the `ctrl+c` that ends `cat` — is the
script's to send once the line is empty (`zyt.line.wait_drained`, then
`zyt.line.write_key`); the manifest says nothing of it.

The application keeps the port while a script runs and pumps it: device → the
script, what the script wrote → device. The device bytes are not fed to the
emulator meanwhile, so a protocol sees a clean stream, and what the script has
to show it writes into the terminal itself. Disconnecting stops the script
first and waits for it, bounded: a script that answers nothing is given up on
and its line is shut under it.

## Progress a program reports

Two things say how far along they are, and each has its own place:

| who says it | kept in | shown in |
| --- | --- | --- |
| a program on the device, OSC 9;4 | `Session::terminal_progress` | a bar left of the status bar buttons, for as long as it reports |
| a script, `zyt.progress` | the run, `Session::run_progress` | the plate of the run, or the row of the run in the panel of what runs |

A script never writes the bar of the status bar: that bar is about a program
the device runs, and any program may say it with no transfer anywhere. A
program saying it runs without saying how far is an empty bar, hinted *No
progress*: a full one reads as nearly done.

What a program said is dropped when a script holding the line starts — the
program no longer reaches the window — when the screen is cleared and on a
disconnect. What a script said goes with its run (`Session::end_run`).

`taskbar::show` hands the Windows shell `Session::outside_progress`: the
script's while one runs, the program's otherwise (`ITaskbarList3`, created once
on the ui thread). Wayland has no such protocol, and the D-Bus interface the
Linux desktops read names the application rather than the window, so nothing
is sent there.

## The panel of what runs

A script holding the line, the programs it started beside the line and file
tasks are one list: a fixed window with a title, a cross, and a row each — what
it does (cut to 64 characters, whole on the pointer), how long it has run, the
button that stops it, and, for a program beside the line, the buttons that drop
the row with its output file and open that file. Row colour is the outcome:
green done, error colour failed or cancelled, none while running.

A script beside the line has no plate over the terminal, so its row carries
what the plate would: the bar of its share between the name and the time —
the share written in it, empty without one, the colour of an error or a pause
— and its stage and speed on the pointer over the name. A script holding the line has no
bar there: its plate shows it.

There is no button that stops everything. The status bar carries one button for
the panel: hovering shows it read-only, pressing leaves it standing with its
buttons.

## The plate of a run

A script holding the line stands on a plate in the middle of the terminal
(`src/ui/transfer.rs`), from its start until its last byte left the port:
while it runs the keyboard reaches nothing and the device is not drawn, so the
window is doing one thing, and the plate is that thing.

```text
┌────────────────────────────────────────┐
│ ZModem · receiving          [ Cancel ] │  the script, which way it carries
│ Receiving · foo.bin · 2 of 5           │  the stage
│ ███████████░░░░░░░░░░░░░░░░░░░░░░░░░░░ │  the share, empty without one
│ 1.2 MiB of 4.0 MiB · 38%               │  the bytes, when said
└────────────────────────────────────────┘
```

| row | from |
| --- | --- |
| the stage | `Session::run_stage`, worded by `ui::transfer` from `locales/`; *Running* before the script said one, *Asked to stop…* once cancelled, *The script is ending…* once it said so, *Sending the last bytes* while the line drains |
| the bar | `Session::run_progress`, in the colour of an error or a pause when the script says so |
| the bytes | `Session::run_bytes`, the share, and the speed `Session::run_speed`, or the speed the script says itself, each way (`Session::run_said_speed`) |

The speed is the growth of the bytes a script says over the last
`rate::PACE_OVER`, measured up to now (`rate::Pace`): a transfer that stopped
moving slows down and then reads as nought, the first report of a file is where
counting starts — a file resumed at its middle is not a megabyte carried in no
time — and a smaller count is the next file, counted afresh. Every protocol
says its bytes the same way, so every one has a speed with nothing of its own
added. It is written by `format::speed`, in bytes, kibibytes or mebibytes a
second as the number climbs. A script that measures its speed itself says it
with `zyt.progress.speed{ received, sent }`, and then that stands in place of
the measured one, each way it said, as it said it: `telnet` says what the
device sent and what the client typed over the last second, and says no share,
so its plate has no bar.

The plate keeps its width and its rows whatever they say, so it does not
move under the pointer as the stage changes. The bar is the one row a run
decides: it stands once the script said a share, an indeterminate one
included, and not before — a script that cannot say how far it is, `scp` below
`-vv` among them, has no bar rather than one that never moves. A long stage
is cut, and whole on the pointer. *Cancel* is the way to stop the run from over the terminal,
disabled while it stops. The plate is not drawn while a dialog of the script or
a menu stands: the run waits on it, and two things in the middle of the window
are one too many. A script beside the line has no plate, the terminal being the
thing its window is doing.

## What can be done with a file

A right click asks the terminal what is under the pointer, every time:

| under the pointer | the menu |
| --- | --- |
| a selection | terminal menu, copying offered |
| nothing selected | terminal menu, copying out of reach and saying why |
| a file of this machine that is there, *Local links* on | the menu of that file or directory |
| a file of this machine, *Local links* off, or one that is not there | copy address, copy text, terminal entries |
| a share `\\server\…`, *Local links* on | open, open with, copy, terminal entries |
| a share, *Local links* off | copy address, copy text, terminal entries |
| any other link | open, open with, copy, terminal entries |

What is a file of this machine is in docs/osc.md, *Links*.

The first plate names what the menu is about and cannot be chosen. The link
comes from the click that opened the menu and nothing else. A program grabbing
the mouse gets the click instead. With a selection standing, the three entries
that write into the terminal are left out: the menu was opened to act on the
selection.

The clipboard carries text and pictures, so a file goes to it as the type
`mime_guess` reads from the name; anything else is said on the desktop, as is
what went when one of them worked.
`Settings.file_menu.copy_limit` caps it — the clipboard hands the whole file to
every asker and a manager keeps a copy. `Settings.file_menu` and
`directory_menu` decide which entries exist and which removals ask first: the
trash does not ask, deleting for good does.

Dropped files (`App::handle_dropped_files`) open the menu of a drop whatever
the session is: hand them to a script, insert the path of the first quoted,
insert its contents when they are text small enough, or pick another script
first — which reopens the same menu, so the files survive the choice. Only a
script whose manifest names `drop` is offered, and it is handed the files as
they are, with no dialog of its own (`Direction::Drop`); with none such, the
entry says so and is out of reach. Only X11, Windows and macOS
deliver drops; Wayland reports none.

Confirmations are one modal (`src/ui/confirm.rs`) answering `Yes`, `No` or
`Pending`.
