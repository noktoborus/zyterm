# Ports

The driver itself — the calls of each platform, the modem lines, the signal
history, enumeration — is `crates/zyt-serial/README.md`. This chapter is what
the application does with it.

## State machine

```
Disconnected ──found + open ok──► Connected
     ▲                               │
     │                     read/write/line error
     └──────── handle dropped ◄──────┘
```

- A fatal error drops the handle at once, and the worker rescans every
  `scan_interval` until the target answers again.
- The target is a `PortId`: USB vendor, product and serial when reported, path
  otherwise. `/dev/ttyUSB1` after a replug is still the same device.
- `write_some` reports how much the driver took and the worker keeps the rest.
  `write_all` would discard a partially written block on a timeout, which on a
  binary transfer means a corrupt block.
- Modem lines are polled every `Settings.lines_interval`. Every poll is a call
  into the driver on the thread that reads the port, so the wait is what watching
  the lines costs.

## What this side drives

| | type | kept in | shared default |
| --- | --- | --- | --- |
| RTS, DTR | `LineHold`: auto, down, up | `PortMemory::holds` | none |
| the direction a press writes | `config::LineForces` | `PortMemory::forces` | `Down` |
| break | `bool` | `SupervisorConfig` | none |
| HUPCL, line parameters | `LineParams` | `PortMemory`, else `Settings.line` | the line parameters only |

A hold is asked of one board, so none of these has a shared default the way the
line parameters do: a device that was never opened leaves both lines to the
driver. The pair is written to the line again on every open, by
`App::holds_for`; a hold that lived as long as the connection would come back up
with a device that was unplugged, which is the moment a board held in reset
would run.

`App::set_line_hold` writes the hold into the file of the device when one of the
two letters is pressed. The direction is a second thing about the device,
because a hold has a third answer — the driver's — and a direction has not:

- `App::line_force` answers with the way a held line is held, and with what was
  written down only while the line is on `Auto`, so the button shows the
  direction the line is on.
- `App::set_line_force` holds a line that is already held the other way at once.
- `App::set_line_hold` writes the direction of a forced hold back into the file.
- `App::toggle_line_hold` is what the left button calls: forced becomes `Auto`,
  `Auto` becomes the hold of that direction.

`HUPCL` has no control at all where the platform has no `termios`:
`zyt_serial::HUPCL_SUPPORTED` is what the settings page asks before drawing the
switch, because a switch that decides nothing looks like it decides something.
`LineParams::flush_on_open` empties both driver queues after the parameters are
on the line, and in that order: what stood in the input queue was framed by
whatever the port was last opened at.

## Giving up on the writing

Five buffers, not three. Clearing fewer would let the rest go out anyway.

| where the bytes stand | emptied by |
| --- | --- |
| terminal answers | `Terminal::forget_output` |
| the output of a running transfer | `cancel_transfer`, which stops it |
| the shared `ByteSwap` | `PortCommand::DiscardOutput` |
| the write buffer local to `Worker::run` | the `discarding` flag, on the next turn |
| the driver queue | `PortHandle::discard_output` |

`Session::discard_output` takes the first two. The transfer is stopped and not
merely drained because it is the one that refills the others: left running, it
would fill the queue again on the next pass.

## Held back

`SupervisorConfig::held` stops the reading and nothing else: the worker writes,
polls the lines and reports the driver queues as before. It waits out
`HELD_BACK` rather than spinning, because nothing else in that pass blocks once
the read is gone, and the wait answers a command at once. It is the state a full
`ByteSwap` reaches by itself (`wait_for_room`), asked for on purpose, which is
why the window of numbers calls both of them held back.

## Queues

`PortStatus` carries the two driver queues beside `pending_output`, which is
everything on this side of the line. The queues say where the bytes are standing
instead: a line nobody reads fills the input one, a line that cannot carry fills
the output one. Neither wakes the interface — they are read in the window of
numbers and in the plate of the signals, and a wake per byte crossing a queue
would be a wake per byte of the line.
