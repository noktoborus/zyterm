# zyt-pty

Local console as a byte stream.

## Scope

- `PtySession::spawn` runs a shell in a pseudo terminal (ConPTY on Windows,
  `openpty` on Unix) through `portable-pty`.
- `read_into(&mut Vec<u8>)` and `write(&[u8])` move whole chunks through a
  swapped buffer, the same data path the serial crate uses.
- `set_read_buffer(bytes)` is the size of that buffer; zero is no limit. It is
  allocated at that size and never grows: the reading thread takes only what
  fits, and a full buffer stops the pty being read. Nothing is thrown away: the
  pipe fills and the program writing into it blocks on its next `write`, so a
  program saying more than the caller takes is slowed to the caller's pace
  rather than kept in memory. `read_buffer()` says how much waits and whether
  the buffer is full.
- `resize(columns, rows)` informs the child about the window size.
- `pending_output()` reports what is still queued for the shell; a pty has no
  driver queue behind it, so only the buffer of this crate counts.
- `is_running()` is true while the shell runs or output is still buffered, and
  `exit_code()` answers the code it ended with. The reading thread asks the
  operating system for that status when the output stops — for a second, in that
  thread — so a console that is over has its code by the time `is_running()`
  turns false. A shell that still runs, and one that closed its handles and
  stayed, answer nothing.
- `default_shell()` reads `SHELL` or `COMSPEC`.
- The shell starts in the directory of this process. `portable-pty` would start
  it in the home directory instead. Nothing names another directory: a caller
  that wants the shell elsewhere goes there itself.

## Boundaries

No emulation, no rendering, no configuration. The caller decides what to do
with the bytes.

## Errors

`Result<T, PtyError>`: `Open`, `Spawn`, `Pipes`, `Resize`, `ThreadStart`,
`Ended`. The pty layer reports untyped errors, so they are wrapped as a boxed
source inside the matching variant.
