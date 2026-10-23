# Plans

Work that is put off: not built, not decided unless it says so. A part that
is built leaves this file in the same commit.

## Telnet: the client

zyterm opens a serial port on a remote server. A discussion of its own, put
off until the server was built. Not decided.

- The telnet of the client is Lua, like the server (`scripts/telnet`).
- Needed: `zyt.net.connect`; a source that is a script (what it reads goes to
  the terminal, what is typed goes to it); the controls of a port reach the
  script, and the modem lines it was told come back to the status bar and the
  plate of the signals; `ports/<key>.yaml` for a key like `telnet:host:2217`;
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
