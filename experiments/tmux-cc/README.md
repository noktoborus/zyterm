# tmux-cc

A real tmux in control mode, and what it says.

## Scope

```sh
cargo run -p tmux-cc -- probe                        # the questions, every tmux on the path
cargo run -p tmux-cc -- probe --tmux tmux-3.0a       # one of them
cargo run -p tmux-cc -- record all                   # every scenario, every tmux
cargo run -p tmux-cc -- record pause --tmux tmux-3.8
cargo run -p tmux-cc -- show crates/zyt-tmux/tests/data/attach-3.3a.txt
```

- tmux runs through `zyt-pty`, the way the program runs a console, on a server
  of its own (`-L zyterm-cc-<pid>-<tag> -f /dev/null`), killed when the run
  ends; no server of the user is touched. The shell of its pane is `sh` with a
  prompt of `$ `, so a recording carries nothing of the machine.
- The versions asked are those the container builds: `tmux` (3.3a of Debian),
  `tmux-3.0a`, `tmux-3.2a`, `tmux-3.4`, `tmux-3.8`, as many as are on the path.
- `probe` asks the questions the design of control mode rests on, a fresh server
  each: the opening pair, both forms of `refresh-client -C`, what tmux answers a
  program (DA1, DA2, `CSI 14t`, `16t`, `18t`, the window size), a command line
  of 256 KB, notifications inside replies and output between the replies of one
  line while two panes flood, the output of a hidden window and of another
  session, the order after `select-window`, `capture-pane -a` and `-S` on the
  alternate screen, a start that fails, `pause-after`, a hyperlink in
  `capture-pane -e`.
- `ask` is what `probe` runs inside a pane: it asks the terminal it stands in,
  which is tmux, and prints `ASK <query> = <answer>`.
- `record` plays a scenario of `scenarios/` and writes
  `crates/zyt-tmux/tests/data/<scenario>-<version>.txt`, the data of the tests
  of `zyt-tmux`; `--out` writes elsewhere. A scenario that `needs` a newer tmux
  is skipped on an older one.
- `show` prints a recording to be read: the lines tmux said, numbered, control
  bytes named, the start of the mode marked, what was sent between them.

## Scenarios

A file of steps, one a line:

| step | |
| --- | --- |
| `needs 3.4` | skipped on an older tmux |
| `send <command line>` | written to tmux as it stands |
| `type <pane> <text>` | `send-keys -H` of the text |
| `until <text>` | read until tmux said it, or the scenario fails |
| `wait <ms>` | read for that long |
| `hold <ms>` | read nothing for that long, so tmux falls behind |

Text takes `\r`, `\n`, `\e`, `\\` and `\xHH`, and `{tmux}` and `{socket}` for the
binary and the server of the run. Every scenario ends with `detach`.

## Recordings

One chunk a line, as the pty gave it: `<` read from tmux, `>` written to it. A
byte below `0x20`, `0x7F` and `\` are written `\r`, `\n`, `\t`, `\e`, `\\` or
`\xHH`; every other byte, UTF-8 included, as it is. A recording is data: it is
made again by `record`, never edited by hand.

## Boundaries

No protocol is parsed here beyond what a question needs: that is the crate
`zyt-tmux`, which replays these recordings in `tests/recordings.rs`. Nothing
depends on this program (`RULE experiments`). It is `#[cfg(unix)]`, as tmux is.
