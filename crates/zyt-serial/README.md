# zyt-serial

Serial port access without a user interface dependency.

## Scope

| | |
| --- | --- |
| `available_ports`, `accessible_ports` | enumerate, classify USB / builtin / bluetooth, and say whether the process may open each one |
| `PortId` | which device a caller means: USB vid/pid/serial, path as fallback |
| `LineParams` | the shape of a character, the flow control, and the two below |
| `ControlLines` | what the modem lines stand at |
| `PortSupervisor` | a worker thread that opens, watches and reopens one device |

The text form of a `PortId` is what it serializes as and what `FromStr` reads
back, so a configuration file carries a key nobody has to decode:

| text | the device |
| --- | --- |
| `usb:vid:pid:serial` | reports an identity |
| `path:/dev/…` | does not |

The serial number is the rest of the text rather than one field among several,
so one containing a colon survives. Text that names no identity is
`PortError::Key` and never a device nobody has.

`flow_control` has a fourth mode beside none, `RTS/CTS` and `XON/XOFF`: `Both`
is the two at once, which the line may carry because they live in different flag
words and neither switches the other on. Windows has one mode at a time and
takes the hardware half of it.

Two settings of `LineParams` are not about the shape of a character:

| | |
| --- | --- |
| `flush_on_open` | throws away what the driver holds in both directions as the port opens, so a session does not begin in the middle of a sentence nobody asked for |
| `hupcl` | the `termios` flag that drops the modem lines when the port closes, which is how the far end is told the session is over. Windows has no `termios` and leaves it to its driver, which is what `HUPCL_SUPPORTED` says: false there, and a caller offers no setting for a flag the platform has not got |

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

```
driver ──► worker ──read what fits──► ByteSwap ──read_into──► caller's spare
                        │             set_read_buffer(bytes)
            no read at all while
            the buffer is full, or set_read_hold(true)
```

| | |
| --- | --- |
| `set_read_buffer(bytes)` | the size of the buffer, zero being no limit. It is allocated at that size and never grows past it, so the worker reads only what fits (`ByteSwap::room`) |
| a smaller size later | memory given back and not only a limit lowered: the buffer in hand is brought to it at once and the caller's at the next swap, so the pair costs the size and not the largest it was ever given |
| a full buffer | stops the port being read |
| `set_read_hold(true)` | the same state asked for on purpose. The worker goes on writing, polling the lines and reporting the queues; only the read is gone, and `Signal::Held` says so in every sample taken while it stands |

Nothing is thrown away either way: the bytes wait in the driver, and a line with
flow control tells the device to wait. A line without it loses what the driver
cannot hold — the same loss such a line always has. The worker gives the turn up
rather than waiting it out, so its commands and the modem lines are still
answered while it is held back.

| call | answers |
| --- | --- |
| `PortStatus::pending_output` | the outgoing buffer plus what the driver took but has not put on the line (`PortHandle::pending_write`: `TIOCOUTQ`, `ClearCommError`) — whether this side is still busy after a transfer |
| `input_queue`, `output_queue` | where the bytes are standing instead: a line nobody reads fills the first, a line that cannot carry fills the second |
| `read_buffer()` | how much waits on the way in, and whether the buffer is full |

Neither queue wakes the caller: they are numbers to read, and a wake per byte
that crossed a queue would be a wake per byte of the line.

`discard_output()` throws away everything on its way out that has not left yet,
in all three places it can be. Clearing one of the three would leave the rest to
go out anyway.

```
caller ──push──► ByteSwap ──► what the driver ──► the driver queue ──► the line
                    │          would not take            │              gone
                    └──────── discard_output() ──────────┘
                              TCFLSH | ClearBuffer::Output
```

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

| | |
| --- | --- |
| `lines_interval` | the wait between two snapshots |
| `DEFAULT_LINES_INTERVAL` | the one a config starts at |
| `set_lines_interval` | changes it while the worker runs |
| `LINES_INTERVAL_RANGE` | what the wait is held inside: a worker polling with no wait at all spends the whole thread on one ioctl |

Every snapshot is a call into the driver on the thread that reads the port, so
the wait is what watching the lines costs: a line looked at closely is a line
read less.

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
| 9 | `Ring` | the peer |

A bar of a picture covers the step between two polls, so what a sample honestly
says is that a signal stood *at some point inside it* — and a level read at one end
of the step cannot say that. `TIOCGICOUNT` can: `LineEdges` is the counters of the
changes on the four lines the peer drives, and `LineSample::with_pulses` raises a
bit whose level said nothing because the count moved. A ring that came and went
between two polls is in the history.

Only those four are counted, because only those are read from the hardware; the
driver knows the two it sets exactly and keeps no count of them. So a line this
side drives that moved and came back inside one step — `RTS` dropped by hardware
flow control and raised again — still leaves nothing to see, and polling faster is
the only answer to that one. Windows keeps no counters at all through the driver
crate, so `line_changes` answers nothing there and a caller draws the levels alone.

The same structure carries `frame`, `parity`, `overrun` and `brk`, which are the
answer to a question nothing else here asks — a line read at the wrong speed is a
line of framing errors and nothing else says so. They are named in `tty::Counters`
and nothing reads them yet.

A sample carries the two queues of the driver beside the bits, as the counts they
are. `LineHistory` keeps `LineScale` with them — the fullest each has been seen —
because how full is full is the one number no call answers: `TIOCINQ` and
`TIOCOUTQ` say what is in a queue and nothing says what fits. The scale only
grows, so a caller drawing a share works it out against the scale of the moment
and a scale that grew does not leave the samples before it drawn too tall.
`PortSupervisor::history` answers it beside the samples.

| call | answers |
| --- | --- |
| `Signal::outgoing()` | which of the two sides drives that signal |
| `LineSample::has(signal)` | what it stood at in that sample |
| `PortSupervisor::history(count, &mut Vec<LineSample>)` | copies the newest `count` of them, oldest first, into a buffer the caller keeps |

`RTS` and `DTR` are taken as `rts_up()` / `dtr_up()` answer them — what the
driver reports, and what was asked for only where the platform cannot read the
line back.

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

```
open ok ──► reading, writing, polling ──► fatal error ──► the handle is dropped
   ▲                                                            │
   └── the PortId of the target is found ◄── rescan every scan_interval
```

The handle goes at once, because a dead handle keeps the device node claimed and
a returning device would appear under a new name.

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
