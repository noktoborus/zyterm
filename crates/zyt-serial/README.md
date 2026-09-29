# zyt-serial

Serial port access without a user interface dependency.

## Scope

- Enumerate ports (`available_ports`, `accessible_ports`), classify USB /
  builtin / bluetooth, report whether the process may open each one.
- `PortId`: USB vid/pid/serial, path as fallback. Its text form —
  `usb:vid:pid:serial` or `path:/dev/…` — is what it serializes as and what
  `FromStr` reads back, so a configuration file carries a key nobody has to
  decode. The serial number is the rest of the text rather than one field among
  several, so one containing a colon survives; text that names no identity is
  `PortError::Key` and never a device nobody has.
- `LineParams` and `ControlLines`. `flow_control` has a fourth mode, `Both`,
  which is `RTS/CTS` and `XON/XOFF` at once: the two live in different flag words
  and neither switches the other on, so a line may carry both. Windows has one
  mode at a time and takes the hardware half of it.
- Two settings of `LineParams` are not about the shape of a character:
  `flush_on_open` throws away what the driver holds in both directions when the
  port opens, so a session does not begin in the middle of a sentence nobody
  asked for, and `hupcl` is the `termios` flag that drops the modem lines when
  the port closes, which is how the far end is told the session is over. Windows
  has no `termios` and leaves the second to its driver, which is what
  `HUPCL_SUPPORTED` says: it is false there, and a caller draws no control for a
  flag the platform has not got.
- `PortSupervisor`: a worker thread that opens, watches and reopens one device.

### Driver

One module per platform, and they are the only place a driver is named:

| | `tty` (linux) | `comm` (windows) |
| --- | --- | --- |
| open, line parameters | `open(2)`, `TCSETS2` | `serialport` |
| claim on the node | `TIOCEXCL` and `flock(2)` | `serialport` |
| read timeout | `poll(2)` on a non-blocking descriptor | driver timeout |
| modem lines | `TIOCMGET`, all six at once | one call per line, four of them |
| break condition | `TIOCSBRK` / `TIOCCBRK` | `SetCommBreak` |
| bytes still on the line | `TIOCOUTQ` | `ClearCommError` |
| bytes not read yet | `TIOCINQ` | `ClearCommError` |
| empty the queues on open | `TCFLSH` | `PurgeComm` |

The `serialport` crate is a dependency of the Windows target only. Linux asks
the descriptor itself: the speed goes in as a number through `TCSETS2`, so a
rate no constant names is asked for like any other, and every flag word is
built from nothing rather than from what stood there — a driver that echoes,
maps line endings or waits for a finished line is a second terminal in front of
the one the caller draws.

Two claims are laid on the device node, because two kinds of program answer
them. `TIOCEXCL` is the kernel's and stops every later `open`; the advisory lock
is the one other terminal programs take. Either refusal is `PortError::Busy`.
The claim is given back before the descriptor closes, so the port this program
dropped is a port it can open again.

### Modem lines

Read in one call where the platform has one: `TIOCMGET` answers all six at once
on Linux, including the two this side drives. So `ControlLines` carries both
what was asked for (`rts_asked`, `dtr_asked`) and what the driver reports
(`rts`, `dtr`) — not the same answer, since opening a port raises both before
anything asked. Where the platform cannot read them back, Windows among them,
the driver side is `None` and `rts_up()` / `dtr_up()` fall back to what was
asked for.

What this side does with each of the two is a `LineHold` and not a level:

| `LineHold` | what the worker writes |
| --- | --- |
| `Auto` | nothing; the driver drives the line |
| `Down` | the line is put down on open and whenever the hold is set |
| `Up` | the line is put up the same way |

The break condition is the third thing this side drives, and it is a `bool`
rather than a `LineHold`: a break is held or it is not, and there is no third
answer a driver could give. `set_break` keeps it the same way a hold is kept, so
a port the worker opened again is still holding it, and `PortStatus::held_break`
says whether it is.

`LineHolds` is the pair of them, which is how a caller keeps and hands them over:
a port is opened with both, and a device that remembers one remembers the other.

`set_rts` and `set_dtr` take one of the three and keep it, so a port the worker
opened again carries it. `Auto` writes nothing at all: a driver takes its lines
over on open and no call hands one back, so a level a forced hold left stands
until the next open. Three states and not two, because a driver raises both
lines on open and hardware flow control drives `RTS` by itself — "not held up"
and "down" are not the same request.

## Data path

Payload bytes do not pass through a channel. The worker reads into a shared
`ByteSwap`; `read_into(&mut Vec<u8>)` swaps that buffer with the caller's spare
one — one lock, one pointer swap, no allocation at all once both buffers stand
at their size. Writes go the same way. Only state changes travel as `PortEvent`.

`set_read_hold(true)` stops the port being read on purpose. The worker goes on
writing, polling the lines and reporting the driver queues; only the read is
gone, and `Signal::Held` says so in every sample taken while it stands. It is the
same state a full buffer reaches by itself, which the next paragraph describes,
and it has the same consequence: nothing is thrown away here, and a line with no
flow control loses what the driver cannot hold.

`set_read_buffer(bytes)` is the size of that buffer; zero is no limit. It is
allocated at that size and never grows: the worker reads only what fits
(`ByteSwap::room`), and a full buffer stops the port being read. Nothing is
thrown away: the bytes wait in the driver, and a line with flow control tells
the device to wait. A line without it loses what the driver cannot hold — the
same loss such a line always has. The worker gives the turn up rather than
waiting it out, so its commands and the modem lines are still answered while it
is held back. `read_buffer()` says how much waits and whether the buffer is
full.

`PortStatus::pending_output` is the outgoing buffer plus what the driver took
but has not put on the line (`PortHandle::pending_write`: `TIOCOUTQ` on Linux,
`ClearCommError` on Windows), which is how a caller tells whether a slow line is
still busy after a transfer. The two queues of the driver are reported on their
own beside it — `input_queue`, `output_queue` — because they answer another
question: not whether this side is done, but where the bytes are standing. A line
nobody reads fills the first and a line that cannot carry fills the second.
Neither of them wakes the caller: they are numbers to read, and a wake per byte
that crossed a queue would be a wake per byte of the line.

Writes hand the bytes over and return. Waiting for the line inside the worker
would stop reading for as long as the write takes, which breaks every protocol
that needs both directions at once.

## Enumeration on Linux

A scan of `/sys/class/tty`. A tty backed by hardware carries a `device` entry
and a virtual console does not, which is the whole filter: it catches `ttyUSB*`,
`ttyACM*`, `ttyS*` and every driver that names its ports otherwise. USB
descriptors are read from the parent device. No udev database is asked for,
because there is none inside a container.

## Modem line polling

`lines_interval` is the wait between two snapshots, `DEFAULT_LINES_INTERVAL` the
one a config starts at and `set_lines_interval` the way to change it while the
worker runs. Every snapshot is a call into the driver on the thread that reads
the port, so the wait is what watching the lines costs: a line looked at closely
is a line read less. The wait is held inside `LINES_INTERVAL_RANGE` — a worker
polling with no wait at all spends the whole thread on one ioctl.

### Signal history

One sample is one poll of the lines, and it is taken whether the snapshot changed
or not — a picture drawn from it is a track over time, and a track carrying a
sample only where something changed has no time on its axis at all. One bit per
signal fits a word, so a history of thousands of samples is a few kibibytes.

| bit | `Signal` | driven by |
| --- | --- | --- |
| 0 | `Sent` | this side: bytes went out since the poll before |
| 1 | `Break` | this side |
| 2 | `Held` | this side: the port is not being read |
| 3 | `Rts` | this side |
| 4 | `Dtr` | this side |
| 5 | `Received` | the peer: bytes came in since the poll before |
| 6 | `Cts` | the peer |
| 7 | `Dsr` | the peer |
| 8 | `Carrier` | the peer |

`Signal::outgoing()` is which of the two a signal is; `LineSample::has(signal)`
is what it stood at; `PortSupervisor::history(count, &mut Vec<LineSample>)` copies
the newest `count` of them, oldest first, into a buffer the caller keeps.

`RTS` and `DTR` are taken as `rts_up()` / `dtr_up()` answer them — what the driver
reports, and what was asked for only where the platform cannot read the line back.

The history has a lock of its own and is not part of `PortStatus`, because the
two are read at different rates: the status is a handful of words asked for every
frame, and this is thousands of bytes asked for only while something draws them.

Nothing wakes the caller for a sample. One is taken four times a second for as
long as a port is open, and a window woken by each would never be idle;
`PortEvent::Lines` still wakes it when a line actually moves.

It is cleared when `set_lines_interval` is given a new value, because the span a
caller writes under a track is `samples × interval` and samples taken at two
different steps cannot share one axis. It is **not** cleared when the device goes
away: the stretch just before a port stopped answering is the one somebody opens a
history to look at. While disconnected the samples keep coming and every signal in
them is down, so a device that was unplugged reads as a gap and not as a splice.

## Reconnect rule

A fatal read, write or line poll drops the handle at once: a dead handle keeps
the device node claimed, so a returning device appears under a new name. The
worker then rescans every `scan_interval` and reopens the port whose `PortId`
matches the target.

## Errors

`Result<T, PortError>`. What a platform module hands over is an
`std::io::Error`, wrapped in `Driver`, `Enumerate` and `Io` with `#[source]` and
never turned into text, so the enum names no driver. Messages are English and
diagnostic only.

## Testing

`PortBackend` and `PortHandle` are public traits. `SystemBackend` implements
them over the OS driver; tests inject a fake backend to drive the state machine
without hardware.

`tests/line.rs` drives the Linux backend against a pseudo terminal. A pty has no
modem lines and no speed, but it is a device node that takes `termios` and
carries bytes, which covers the claim on the node, the raw line, the read
timeout and what a read and a write hand over.
