# Journals

The block *Debugging* of the settings writes journals into a directory:
`debug.journal_dir`, the working directory of the process when empty, and a
relative one under that (`DebugSettings::directory`).

| setting | file | what is in it |
| --- | --- | --- |
| `debug.journal` | `<date>-<time>-<pid>-line.log` | every byte of the line, both ways |
| `debug.messages`, `debug.messages_others` | `<date>-<time>-<pid>-window.log` | the messages of the `log` crate, from those levels up |

The date and the time are those of the moment the journal opens, in local time:
`20261113-142530-4321-line.log`. Every opening is a file of its own — at the
start of a run with the switch on, at the switch turned on (for the messages:
a level turned on from off), at the directory changed while it is on. A file
that is already there is written on at its end. Nothing is ever removed, and
there is no limit of size.

A journal that cannot be opened is said (`AppError::Journal`) and the line goes
on without one.

## The journal of the line

`Lease` of `src/line.rs` is the one place every byte of the line passes, both
ways, whoever holds the line: the screen, or a script that runs a transfer. It
hands each chunk to `LineJournal::record` before tmux or the terminal read it,
so what the journal holds is what the source said and what went to it.

```
# zyterm 1.9.4 journal of the line, process 4321, opened 2026-11-13 14:25:30 +0300, at 0.213907 seconds of the process, windows x86_64
# source: serial COM3 115200 8N1
# size: 120x40
< 0.012345 screen \eP1000p%begin 1792047896 257 0\r\n
> 0.013001 screen list-sessions -F '#{session_id} #{session_name}'\n
# 9.500000 source: none
```

| line | |
| --- | --- |
| `# …` | what the session is: the head, the source and the size at opening, a source opened or let go of later, with its second |
| `< <second> <owner> <bytes>` | one chunk from the source, as the source gave it |
| `> <second> <owner> <bytes>` | one chunk written to the source |

The second counts from the start of the process, in every journal of the run,
so the lines of the two journals can be laid side by side; the head says at
which second the journal opened. The owner is `screen` or `run`. The bytes are escaped as the recorded sessions of `zyt-tmux` are
(`journal::escape`): `\r`, `\n`, `\t`, `\e`, `\\`, `\xHH` for any other control
byte and for a byte that is no part of UTF-8; text stays text. A chunk is one
line, so the edges of the chunks stay where the line put them: a sequence cut
across two of them is cut in the journal too.

The chunks are formatted on the thread that holds the line and handed through a
`ByteSwap` to the thread `zyterm-journal`, which writes them; a slow disk holds
up the journal and not the line. Closing the journal writes what is still
waiting; at the exit that is done by the thread of the exit, within its limit
(`exit::LIMIT`).

The journal holds everything typed and received, passwords too
(`RULE journal.private`).

## The journal of messages

`journal::install_logger` is the logger of the process. It writes to stderr as
`RUST_LOG` says, `info` when it says nothing, as `env_logger` did on its own;
on Windows a release build has no console, so stderr is seen only when the
program is started from one. On top of that, while a level is not off, it
writes to the journal of messages:

| level | the messages of |
| --- | --- |
| `debug.messages` | `zyterm` and every crate `zyt_*` |
| `debug.messages_others` | every other crate: `winit`, `egui`, `wgpu`, `egui_plate_menu`, … |

The two stand apart: the messages of the libraries are written with those of
zyterm off, and the other way round.

```
# zyterm 1.9.4 journal of messages, process 4321, opened 2026-11-13 14:25:30 +0300, at 0.214001 seconds of the process, windows x86_64
1.502113 DEBUG zyterm::tmux tmux: the layout of @1: …
1.503004 INFO winit::platform_impl::windows::window …
```

A message is one line — its second, its level, its target, its text with the
line breaks escaped. The `log` crate builds no message nobody wants:
`log::set_max_level` follows the widest of stderr and the two levels.


## Playing a journal

`crates/zyt-tmux/tests/journal.rs` plays the chunks of the screen in a journal
of the line through the splitter and the client of tmux, as the window plays
the line: what came goes through the splitter, and what went while control
mode stood is queued in the client as sent, in the order it went; a command the
player does not know by name is queued all the same, so the replies still find
their commands. It prints every event and says the replies that never came:

```
ZYTERM_JOURNAL=20261113-142530-4321-line.log \
    cargo test -p zyt-tmux --test journal -- --ignored --nocapture
```

The same file plays every recorded session of `tests/data` written as a
journal, a shell and `tmux -CC` typed before it, on every run of the tests. A
journal that shows a fault becomes a recorded session of its own once it is
read and what is private is taken out (`RULE journal.private`).
