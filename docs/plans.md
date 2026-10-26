# Plans

Work that is put off: not built, not decided unless it says so. A part that
is built leaves this file in the same commit.

## The order of work

Set on 2026-10-26, by the user's word. Each step is a change of its own.

1. Ports: the line as the driver has it (below).

The rest of this file waits behind it.

## Ports: the line as the driver has it

Decided on 2026-10-26 with the user. Not built. First in the order of work.

The bug: `PortSupervisor::set_params` only sends a command, and
`PortStatus.params` changes when the worker takes it, up to 100 ms later. The
telnet server answers SET-BAUDRATE with `port.params()` read at once, so the
client is told the old speed. Several SET commands in one packet each start
from the old status, and the last one puts the old speed back.

### What the driver has

- `PortHandle::set_params` and the open read the line back after setting it:
  `TCGETS2` after `TCSETS2` on Linux, the getters of `serialport` on Windows.
  They give the `LineParams` the driver keeps.
- What `LineParams` cannot say (mark and space parity, a size it has not
  got) is ignored.
- `PortStatus.params` is written by the worker alone, with what it read
  back. When nothing was read (no port, the driver gave nothing), it is what
  was asked.
- The worker reads the line again with the modem lines, every
  `lines_interval`. A change made by another process (`stty -F` as root) is
  taken into the status and is a desktop notification. It is not written to
  the settings.
- What the device does with the line coding is the kernel's: `cdc_acm` keeps
  the asked speed in termios even when the device refused it.

### `apply_params`

- `PortSupervisor::apply_params(params) -> Result<LineParams>`: the command
  `SetParams` carries a channel for the answer, and the caller waits for it
  with a limit (about 1 s). The answer is what the driver has after the set.
- `PortError::Timeout` when the limit passes, `PortError::WorkerStopped` when
  the worker is gone, the error of `TCSETS2` when the driver refused.
- A closed port answers `PortError::Disconnected`, and the request is not
  kept: the port opens with the line it had.
- A line the driver refuses is ignored, by the user's word: the port stays
  open, or is opened, with what the driver has, and the status has that
  line. `SetParams` no longer reopens the port, and `open` no longer fails
  on `TCSETS2`. So a port is never opened and closed every `scan_interval`
  over its line: each try would raise and drop DTR and RTS, which resets
  many boards.
- The refusal is one desktop notification, the first time; a refusal of the
  same line again says nothing. `apply_params` still gives the error to its
  caller.
- The window does not wait (`RULE ui.thread`): it sends the same command
  with no channel, as now.
- Commands are taken in order; the last one wins, by the user's word.
- The modem lines the same way: setting RTS or DTR answers the lines read
  back (`TIOCMGET` after the set), and `PortStatus.lines` takes them at
  once, not at the next `lines_interval`.

### Scripts

- `zyt.port.set_params{…}` waits for the answer and gives a table of the
  line the driver has, or `nil` and the error. A script that tests the old
  `true` still works. `set_rts` and `set_dtr` give the lines read back.
- A call waits up to about 20 ms, 100 ms while the read buffer is full.
  Cancel waits for the call to return; Force does not.
- `ScriptPort::set_params`, `zyt_script::Line::set_port` and the binding of
  `api/port.rs` give the line, not a `bool`. A partial table starts from the
  status, which now has what the last call set.
- `Changed.params` gets what the driver has. The window writes it down when
  the run ends, as now, by the user's word.
- `scripts/telnet/init.lua` answers each SET command with what the call gave,
  or with `port.params()` when it failed: the value it has, as RFC 2217 asks.
  A client that sees an answer unlike its request takes it as refused. A
  closed port is told by NOTIFY-MODEMSTATE, all lines down, as now.
- The same in SET-CONTROL (code 5): DTR and RTS on or off are answered with
  the line read back, not the code asked; a query of DTR or RTS (7, 10)
  reads `port.lines()`, which has them now.

### The window

- The status bar shows the summary of the line the driver has
  (`status.params`). When it is not what was asked, in any field the summary
  shows, it is drawn in `warn_fg_color`.
- The menu of the line parameters, opened while the two differ, carries a
  `notice` with the line the port has now.
- The lists and the settings page show what was asked, and the settings file
  keeps what was asked.
- The new texts (the notice, the notification) go to both locales of
  `locales/app.yml`.

### Tests

- The worker: `set_params` then `status().params` is the line read back;
  `apply_params` on a closed port is `Disconnected` and keeps nothing; a
  refused line is an error to the caller, the port stays open, and one
  notification is said for it.
- A fake port that rounds the speed: the status has the rounded one, the
  settings file the asked one.
- The telnet server: four SET commands in one packet end on the asked line;
  DTR set and asked for in one packet answers the new level.

### Documents to change with the code

- `docs/scripts.md`: what `zyt.port.set_params`, `set_rts` and `set_dtr`
  give, and that they wait.
- `docs/ports.md`, `crates/zyt-serial/README.md`: `apply_params`, the read
  back, who writes `PortStatus.params`.
- `docs/ui.md`: the colour of the line in the status bar and the notice.

## Telnet: the client

zyterm opens a serial port on a remote server. A discussion of its own, put
off until the server was built. Not decided.

- The telnet of the client is Lua, like the server (`scripts/telnet`).
- Needed: `zyt.net.connect`; a source that is a script (what it reads goes to
  the terminal, what is typed goes to it); the controls of a port reach the
  script, and the modem lines it was told come back to the status bar and the
  plate of the signals; `lines/<key>.yaml` for a key like `telnet:host:2217`;
  a lost connection is `Lost` and is tried again (`RULE port.fatal`).
- Two modes. `tcp`: bytes as they are, no line control (Moxa TCP Server,
  ser2net raw, socat). `telnet`: telnet frames, line control when the server
  agrees to option 44 (ser2net `telnet(rfc2217)`, Moxa RFC 2217 mode, pyserial
  `rfc2217_server.py`). A flag may make a refusal of option 44 an error.
- Without option 44 the controls of a port answer "not supported"; HOLD stops
  reading the socket. An answer is waited for with a limit; no answer is no
  error.
- Rejected: netcat in a console. A pseudo terminal changes bytes, ConPTY
  rewrites them, and a console has no controls of a port. A command as a
  transport, with pipes (`ssh gw -W host:2217`), may come later for tunnels.
- Interface: "Connect over network…" in the source menu, a form of address,
  port and mode; the row of the menu with a network icon (`RULE icon.search`)
  and `host:port`; the settings of a console and of a port; the controls of
  the status bar change after the negotiation; new `AppError` variants in
  `locales/app.yml`; a network source is not trusted by default
  (`docs/osc.md`).
- Open: where to add a network port (proposal: the menu to add, the settings
  to edit); missing controls hidden or disabled with a hint; a command line
  argument `--port telnet://host:2217`.
- Moxa Real COM is a closed protocol of two connections a port; NPort 5000
  and 6000 have an RFC 2217 mode, so a client of RFC 2217 serves them too.

## Telnet: the server

- No test drives a client of RFC 2217 against a port, and no window test
  reads the status bar while a client sets the line.
- On a console the server gives a shell of this user to whoever connects,
  with no password. Not now.

## The terminal

- `?2048`: the terminal tells a program its size in the stream of bytes
  (`CSI ? 2048 h`; `CSI 48 ; rows ; columns ; height ; width t` at once and on
  each change). For a serial line, which has no `SIGWINCH`. `alacritty_terminal`
  does not know it; the screen may answer for it. Not checked.
- DECREQTPARM (`CSI x`) answered with the parameters of the port.

## Threads

Moved out of the order of work on 2026-10-22, by the user's word.

- The disk and the dialogs of files in the background: saving the settings
  and the dialogs of files still run on the thread of the window.
- The guard: one thread watches the others by their counts of beats and says
  when one stands for 2 s; it mends nothing. The window counts in `logic` and
  `ui`, the port each turn of its worker, the screen each turn. No count for
  a script while its calls do not turn, nor for a pty.
- The async API of scripts: an executor in the thread of a script and every
  call of `zyt` async. Not needed while `zyt.wait` and Force do the work.
- Slices and a pool for the screen. Measured on 2026-10-22, release build,
  100 000 lines of 200 columns: a search that finds nothing 457 ms, a resize
  60 to 74 ms, a sixel of 1600 × 1200 16 ms. They wait for a case.
- A hung port: the session leaves a worker stuck in the driver and opens the
  port in a new one (`PortSupervisor::spawn_shared`); it needs the guard.
- To check: `close()` after `tcflush(TCOFLUSH)` on a USB adapter; `kill` of
  the group of a ConPTY program and the job object of a script on Windows;
  the cost of a snapshot of a large window.

## Size

The goal: a smaller code base with the same features (`AGENTS.md`, *Size*;
`docs/tools.md`). Put off on 2026-10-23 until the telnet server was done.

- The tools first. `rustloc` is missing from `/opt/packages.sh`, so `size`
  and `files` do not run. `clones` printed "0 clones" with no Java, and the
  lexer of PMD skips 15 files on a string that ends in `\\` before another
  string on the line: give PMD a copy where `\\` is two other characters.
  `cover` mangles names and counts a closure as a function.
- Reports to add: `refs` from `rust-analyzer lsif` (items nobody uses,
  functions of one caller, items only tests use) and `lints` (warnings of
  clippy by lint).
- Then, one commit each: items nobody uses; functions that only pass a call
  on; fixes of clippy; the clones, the largest first (`chunks.rs` of
  `zyt-pty` and `rxbuf.rs` of `zyt-serial` share about 250 lines; the setup of
  the tests); the largest files (`src/app.rs`, `view.rs` of `zyt-term-egui`,
  `src/ui/settings.rs`, `emulator.rs` of `zyt-term`).
- Open: whether `experiments/*` is in scope; `luacheck` for Lua or CPD alone;
  the branch.
