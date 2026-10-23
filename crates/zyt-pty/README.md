# zyt-pty

Local console as a byte stream.

## Scope

| call | what it does |
| --- | --- |
| `PtySession::spawn` | runs a shell in a pseudo terminal (ConPTY on Windows, `openpty` on Unix) through `portable-pty` |
| `read_into(&mut Vec<u8>)`, `write(&[u8])` | move whole chunks through a swapped buffer, the same data path the serial crate uses |
| `set_read_buffer(bytes)` | the size of that buffer, zero being no limit |
| `read_buffer()` | how much waits, and whether the buffer is full |
| `set_read_hold(bool)`, `read_hold()` | stops the pty being read on purpose, and says whether it is stopped |
| `resize(columns, rows, cell)` | informs the child about the window size, in cells and in the pixels they come to |
| `pending_output()` | what is still queued for the shell; a pty has no driver queue behind it, so only the buffer of this crate counts |
| `is_running()` | true while the shell runs or output is still buffered |
| `exit_code()` | the code it ended with |
| `default_shell()` | reads `SHELL` or `COMSPEC` |

The buffer is allocated at its size and never grows past it: the reading thread
takes only what fits, and a full buffer stops the pty being read. A smaller size
given later is memory given back rather than only a limit lowered — the buffer
in hand is brought to it at once and the caller's at the next swap, so the pair
costs the size and not the largest it was ever given.

Nothing is thrown away. The pipe fills and the program writing into it blocks on
its next `write`, so a program saying more than the caller takes is slowed to
the caller's pace rather than kept in memory.

The reading waits on a condition variable while it is held, so a console held
for an hour costs no wakeups and begins again on the call that lets it go rather
than on the next look. A session that is ending is never held by it. The read
already waiting on the pty is answered once more — the flag is looked at between
two reads, and a read of a pty waits for as long as the program is quiet — so
one chunk may still arrive after the hold is set, and everything after it waits.

A session that ends stops reading at once, even while something its program
left behind still holds the pty. A read of a pty ends only when the last handle
of the other side is closed, and a daemon of the program — a server of tmux
keeps the terminal of its client — may hold one for ever. So on Unix the reading
waits for the pty with `poll`, `HELD_BACK` at a time, and looks at the session
between two waits. The reading thread holds the master, so the descriptor it
waits on stays the master's; when the thread ends the master is closed, and the
daemon sees its terminal go.

`shutdown`, and a drop of the session, kill the program and its process group
and wait for nothing. The reading thread reaps the program.

The reading thread asks the operating system for the exit status when the output
stops, for a second, in that thread, so a console that is over has its code by
the time `is_running()` turns false. A shell that still runs, and one that
closed its handles and stayed, answer nothing.

The window size carries pixels beside the cells: `PtyConfig::cell` is how large
one cell is, and the size told to the kernel is that multiplied by the grid. A
program that draws reads those fields of the `winsize` (`TIOCGWINSZ`) rather
than asking the terminal in a sequence it may get no answer to, so a caller that
knows the size of a cell says it here as well as in its emulation. A cell of
nought pixels is a caller that does not know, and nought is what the field
carries then.

`PtyConfig::environment` is the variables the program is started with, over the
environment of this process, and `TERM` is set before them so a caller may
replace it.

The shell starts in the directory of this process; `portable-pty` would start it
in the home directory instead. Nothing names another directory: a caller that
wants the shell elsewhere goes there itself.

## Boundaries

No emulation, no rendering, no configuration. The caller decides what to do
with the bytes.

## Errors

Holding the reading is the one call here that cannot fail: it is a flag the
reading looks at, and a session that has ended reads no more whatever it says.

`Result<T, PtyError>`: `Open`, `Spawn`, `Pipes`, `Resize`, `ThreadStart`,
`Ended`. The pty layer reports untyped errors, so they are wrapped as a boxed
source inside the matching variant.
