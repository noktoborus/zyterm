# Data path

The shape of the path is in `ARCHITECTURE.md`. This chapter is the size of the
buffer, how long the bytes wait in it, and what asks for a frame.

## Read buffer

`Settings.read_buffer` (kibibytes) is the size of the filled buffer.
`App::apply_read_buffer` turns it into bytes, `None` becomes nought, which is no
limit, and the size reaches `ByteSwap::set_size` through the worker of the
source. The buffer never grows past it, so a source that fills it stops being
read instead:

| buffer | what the worker does |
| --- | --- |
| room left | reads `room()` bytes, pushes, wakes the window |
| full | `wait_for_room`, no read at all |

The worker reads `room()` and no more, and `push` takes only what fits and says
how much that was, so a chunk never carries the buffer past its size. The spare
the ui thread hands back is brought to the size before it is filled again
(`fit`): one allocation per buffer, and none after that.

The size is followed downwards as well, because a lowered setting is memory the
window was asked to give back. `set_size` resizes the buffer in hand at once,
and the one the ui thread holds follows at the next swap, which is the one
moment it holds nothing. The first size of a pair is not the setting —
`SupervisorConfig::buffer_capacity` for a port, what `PtySession::spawn` picks
for a console — so that memory is exactly what the setting is asked to free.

Nothing is dropped. The bytes wait where they were produced: in the pty pipe,
whose program blocks on its next `write`, and in the port driver, which asserts
flow control if the line has any. A line without flow control loses what the
driver cannot hold.

The wait times out and is taken again, so a worker that is no longer wanted can
notice: the pty thread checks `running`, the port worker answers its commands
and polls modem lines while held back.

## The ladder of waits

How long the bytes wait before the ui thread takes them is a ladder and not one
number, because a console being typed into and a console pouring out a build log
are not the same thing. `Settings.read_steps` is a list of
`ReadStep { speed, interval }`: a row holds up to its speed in kibibytes a
second, and the slowest row the source has not passed is the one in force.

`Settings.read_above` (`ReadAbove { interval, linear }`) says what happens past
the last row:

| | the wait above the ladder |
| --- | --- |
| `linear` | grows with the speed, up to `interval`, meeting the last row at its speed |
| not `linear` | `interval` from the first byte past the ladder |
| last row waits for nothing, or no rows at all | `interval` |

The rows are in whatever order they were written in. `config::read_pace` asks
for the slowest row the source has not passed, so the order changes nothing. The
page that writes them puts nothing right and marks instead:
`config::read_step_amiss` warns about a row that does not read as a step of a
ladder — a speed the row above holds, or a shorter wait than that row's —
and `config::read_above_amiss` warns about a wait past the ladder shorter than
its last row. A page that sorted itself would move the row under the hand typing
it, and one that cut a value to fit would decide which of two numbers was the
mistake.

`Session::byte_rate` counts what arrived in the last second (`crate::rate`), and
`App::apply_read_interval` reads the ladder with it every frame
(`config::read_wait`). The window of numbers shows both the speed and the wait,
so a row can be set against what it does.

A wait holds back the reading and nothing else: the interface is drawn whenever
the toolkit draws it. A transfer lifts the wait, since its protocol answers
block by block.

## Frames and idle cost

The window repaints only when there is a reason. Device output, connection
events and transfer progress wake it through the `notify` callback of the
workers; an idle terminal asks for no frames.

| what ticks | interval |
| --- | --- |
| the menu of the sources, while open | 1 s |
| a running transfer | 200 ms |
| the plate of the times, while open | 10 s |
| the plate of the signals, while open | `lines_interval` |
| the window of numbers, while open | 500 ms |
| a running file task | slow tick, for its elapsed time |

`notify` calls `request_repaint_after(read_interval)` rather than
`request_repaint`, so a hundred arrivals inside one interval wake the window
once. The interval is the one the ladder answers at the speed of the moment.

The desktop colour change arrives on a `dark_light` channel, which nobody would
be waiting on in an idle window, so a thread of its own marks it and asks for a
repaint. `ThemeWatcher::poll` then asks the platform again on the ui thread, so
the colour is decided in one place.

Waiting for the toolkit to answer a clipboard request is the one place that
would spin, so it asks for a frame each time and gives up after 400 ms. Writer
threads wait on condition variables rather than polling.
