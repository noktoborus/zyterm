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
- `LineParams` and `ControlLines`.
- `PortSupervisor`: a worker thread that opens, watches and reopens one device.

### Driver

One module per platform, and they are the only place a driver is named:

| | `tty` (linux) | `comm` (windows) |
| --- | --- | --- |
| open, line parameters | `open(2)`, `TCSETS2` | `serialport` |
| claim on the node | `TIOCEXCL` and `flock(2)` | `serialport` |
| read timeout | `poll(2)` on a non-blocking descriptor | driver timeout |
| modem lines | `TIOCMGET`, all six at once | one call per line, four of them |
| bytes still on the line | `TIOCOUTQ` | `ClearCommError` |

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

## Data path

Payload bytes do not pass through a channel. The worker reads into a shared
`ByteSwap`; `read_into(&mut Vec<u8>)` swaps that buffer with the caller's spare
one — one lock, one pointer swap, no allocation at all once both buffers stand
at their size. Writes go the same way. Only state changes travel as `PortEvent`.

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
still busy after a transfer.

Writes hand the bytes over and return. Waiting for the line inside the worker
would stop reading for as long as the write takes, which breaks every protocol
that needs both directions at once.

## Enumeration on Linux

A scan of `/sys/class/tty`. A tty backed by hardware carries a `device` entry
and a virtual console does not, which is the whole filter: it catches `ttyUSB*`,
`ttyACM*`, `ttyS*` and every driver that names its ports otherwise. USB
descriptors are read from the parent device. No udev database is asked for,
because there is none inside a container.

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
