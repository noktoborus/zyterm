# flood

Standard output, filled faster and faster.

## Scope

- Writes lines at a speed it names, holds that speed for ten seconds, doubles
  it, and stops after ten megabytes a second. From 1 KiB/s that is fifteen steps
  and two and a half minutes.
- Every line carries its number and the speed it was written at, so what a
  terminal dropped, reordered or fell behind on is read off the screen rather
  than guessed at.
- A reader that cannot keep up does not say so: it stops taking bytes, the pipe
  fills, and `write` stops returning. That wait is the delay, and it is written
  down in two places. The first line that goes out after a wait carries how long
  the wait was, so it stands in the stream where it happened:

  ```
  00003696     65536 B/s ................................ held back 700 ms
  ```

  Standard error then carries one line per step — what it asked for, what it
  managed, and how long it stood still altogether:

  ```
      16384 B/s asked,     16117 B/s managed,    48384 bytes in   3.0 s
      65536 B/s asked,     20167 B/s managed,    65553 bytes in   3.3 s, held back   3.0 s in 3 writes, longest 2200 ms
  ```

  A write nobody holds back is a copy and a system call, so anything past a
  millisecond is counted as standing still rather than as a slow write.
- What is owed is worked out from the time that has passed, not from the tick
  before, so a write that took longer than its turn is made up for by the next
  one and the speed over a step is the speed that was asked for.
- It is primitive on purpose: no dependencies, one thread, two numbers on the
  command line.

## Boundaries

It knows nothing of this application. It writes bytes to standard output and
stops when that output is gone — a reader that closed the pipe ends it, and so
does `^C`. It measures the delay from its own side only: how long its writes
took to return. What the reader did with the bytes afterwards is the reader's
to say.

## Running

```sh
cargo run -p flood
cargo run -p flood -- 1000000 5
```

The first number is the speed it stops after, in bytes a second; the second is
how long one speed is held, in seconds.

Run it in a console of this application with the window of numbers open, and
watch the speed and the update interval move together: the ladder of
`Settings.read_steps` is what decides how often what it writes is taken.
