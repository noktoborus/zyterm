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

The screen leaves a reading until later while a selection stands on the page,
until it goes (`docs/ui.md`). The bytes gather where they were produced and the
buffer is what bounds the gathering.

The worker reads `room()` and no more, and `push` takes only what fits and says
how much that was, so a chunk never carries the buffer past its size. The spare
the screen hands back is brought to the size before it is filled again
(`fit`): one allocation per buffer, and none after that.

The size is followed downwards as well, because a lowered setting is memory the
window was asked to give back. `set_size` resizes the buffer in hand at once,
and the one the screen holds follows at the next swap, which is the one
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

## Frames and idle cost

The window repaints only when there is a reason. Device output wakes the
screen, which asks the window for a frame when it put something new on its
board; connection events and transfer progress wake it through the `notify`
callback. An idle terminal asks for no frames, and the toolkit draws no more
than a frame per refresh of the display however often it is asked.

| what ticks | interval |
| --- | --- |
| the menu of the sources, while open | 1 s |
| a running transfer | 200 ms |
| the plate of the times, while open | 10 s; `RATE_TICK` while the source talks, so the speed beside what it carried falls to nought once it stops |
| the plate of a run, while it shows a speed | `ui::transfer::SPEED_TICK` |
| the plate of the signals, while open | `lines_interval` |
| the debug window, while open | 500 ms |
| a running file task | slow tick, for its elapsed time |

The desktop colour change arrives on a `dark_light` channel, which nobody would
be waiting on in an idle window, so a thread of its own marks it and asks for a
repaint. `ThemeWatcher::poll` then asks the platform again on the ui thread, so
the colour is decided in one place.

Waiting for the toolkit to answer a clipboard request is the one place that
would spin, so it asks for a frame each time and gives up after 400 ms. Writer
threads wait on condition variables rather than polling.
