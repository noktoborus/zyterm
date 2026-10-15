# pty-throttle

A command behind a serial line of a given speed.

## Scope

```sh
cargo run -p pty-throttle -- -b 9600 -- tmux -CC new        # tmux in control mode at 9600
cargo run -p pty-throttle -- -b 115200 -- vim               # a full screen program
cargo run -p pty-throttle -- -b 57600 -f 7E1                # $SHELL, 7 data bits
cargo run -p pty-throttle -- -b 300 -- cargo run -p flood   # a flood through a narrow line
```

It stands between the terminal it runs in and a command, as a serial line
stands between a terminal and a device:

```
zyterm ──console──► pty-throttle ──pseudo terminal──► command
         keys ───────► up   line ────────────────────►
         screen ◄───── down line ◄────────────────────
```

- Each direction is a queue that gives a byte one byte time after the one
  before it: `bits of the frame / baud` — 10 bits for `8N1`, 11 for `7E2`. A
  byte arrives when its last stop bit has gone, and a line that stood idle
  credits nothing to the bytes after it.
- Each queue holds `--queue` bytes. A full one stops its side being read, so a
  command writing faster than the line waits in its `write`, as on a serial
  console with a tty driver on the far side.
- A frame of fewer than eight data bits loses the high bits, as the wire does:
  UTF-8 through `7E1` arrives broken.

What the terminal and the command need for full screen programs, the alternate
screen among them:

| what | how |
| --- | --- |
| the terminal this runs in | raw mode: no echo, no line editing, no signals from keys, no translation. `^C`, `^Z`, `^S`, `^Q` go to the command as bytes, and its terminal does with them what its own settings say. Put back on exit |
| the terminal of the command | a pseudo terminal that starts from the settings of the outer one — erase key, interrupt key, flow control — or from the defaults of the kernel when there is none |
| the session | the command leads a session of its own with the pseudo terminal as its controlling terminal, so it gets `SIGWINCH` and `^C`, and `/dev/tty` opens |
| the size | read from the outer terminal at the start and on every `SIGWINCH`, and given to the pseudo terminal, which tells the command |
| the speed | the pseudo terminal is told the nearest speed `termios` names, so `stty` and `ospeed` read it; an arbitrary speed would read back as `0`, a line that hung up. The frame is not told: a pseudo terminal of Linux keeps `cs8` and no parity whatever it is told |
| `TERM`, the environment | passed as they are |
| the end | what the command left switched on is switched off when it is cut half way: the alternate screen, a hidden cursor, mouse reports, bracketed paste, focus reports, the keys of the application mode, a scroll region, the character set, attributes (`tty::RESET`; `--no-reset` keeps them) |

How it ends:

| | |
| --- | --- |
| the command closes its terminal | what waits in the down line goes out first, at its speed; the code of the command is the code |
| `^]` twice in a row | at once: what waits is thrown away, the command gets `SIGHUP` |
| `SIGTERM`, `SIGHUP`, `SIGINT` | the same |

Standard error carries a line before the command starts — the speed, the
frame, the bytes a second — and one after it ends: what went each way, how
fast, how it ended. `--quiet` says neither.

## Boundaries

`rustix` for every call of the system and `signal-hook` for the signals; no
`libc` and no `unsafe`. The command is started through this program again
(`--lead-session`): in a fresh process `setsid` and `TIOCSCTTY` are safe calls
of `rustix`, and done in the parent before `exec` they would need `pre_exec`.

Unix only, as a pseudo terminal is: on Windows the binary only says so, and its
tests — the unit tests and `tests/line.rs` — are `cfg(unix)` and not built.
Nothing depends on it (`RULE experiments`).
