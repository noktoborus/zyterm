# The plates over the terminal

Two plates stand over the output and are raised the same way: the pointer on a
control of the status bar raises one, a press pins it, and a press takes it
down. `data_plate_hidden` and `signals_hidden` are the same trick in both — the
press that unpins lands on the button the pointer is resting on, which would
raise the plate again on the same frame, so it is held down until the pointer
leaves.

## The plate of the times

Hovering the connection label shows it; the right button pins it
(`data_plate_pinned`). The left
button is already the session menu.

Four rows. Three are moments, in the order they happened, each with a clock
reading and how long ago:

| row | |
| --- | --- |
| input stopped | `Session::last_written` |
| data began | `first_data`, cleared by every write |
| data ended | `last_data` |
| what the stretch carried | `Session::answered`: bytes since the last write, put back to nothing by the next one, so two answers are never counted as one; beside it, how fast the source talks now (`Session::byte_rate`, `format::speed`), nought on a quiet line |

`format::volume` writes the fourth, in bytes up to `VOLUME_STEP` and in
kibibytes above it: past that the last three digits are noise. It is named
*Received*, which is what it is: what came from the source. `format::speed`
writes the speed in the unit its number reads best in — bytes, kibibytes or
mebibytes a second — since nothing beside it is read against a unit;
`format::rate` keeps kibibytes a second where a row of the settings is. Data that began
and ended at one moment says how long ago once. `session::Moment` keeps a
timestamp and a monotonic reading both, because a clock put right mid-session
would make ten seconds ago come out as an hour.

Above the rows stands the track, `statusbar::delta_track`:

```
    [span]        [span]            [span]
  *───────────*─────────────*──────────────────────►
  end of      data          data              now
  input       began         ended
```

- `track_marks` builds one mark per *place*. Moments that fall together share a
  dot and stack their names; data that began and ended at one moment is named by
  its end alone.
- The stretch leaving a mark is the wait before the answer (warning colour) or
  the data itself (selection colour), decided by `Mark.began`.
- It is not a scale: every finished stretch takes `STRETCH_SHARE` of the track
  and the silence takes the rest, because a 50 ms wait beside a five minute
  silence would otherwise be a six-thousandth of the track. The true span is
  written above each stretch.
- A label that would collide with its neighbour is dropped; two names run
  together say less than one.

## The plate of the signals

What a sample holds, which bits there are, how pulses are caught and when the
history is cleared is `crates/zyt-serial/README.md`.

`ui::signals::edge` decides whether it is drawn and against which edge:
`signals_pinned` when it is pinned, the foot while `signals_hovered &&
!signals_hidden`, and nothing on a window with no source, which has no history
to put in a track. `statusbar::signals_button` is the button at the head of the
line controls, and `AppCommand::ToggleSignals` (`view.toggle_signals`, terminal
context) is the other way in, for hands that are typing. Both take one step of
`ui::plates::next_pin`:

| press | the button | the plate |
| --- | --- | --- |
| first | pressed | pinned at the foot |
| second | pressed | pinned at the head |
| third | released | gone, and held down until the pointer leaves |

The pointer alone raises it at the foot whatever stood before. It once stood
against the edge the terminal cursor was furthest from, and a cursor moving about
the middle of the page sent it from edge to edge faster than it could be read.

It is one button and not the letters beside it, because every one of those is
worked — `BRK` and `HOLD` are turned over, `statusbar::driven_line` flips a line
with the left button and opens `Choice::LineControl` with the right, the flow
control opens its list — and a plate rising from them would rise on every
press.

### Which rows a source has

`ui::signals::stands` answers it.

| source | rows |
| --- | --- |
| a port | the `ShownLines` of its own file, and both driver queues |
| a console | `ui::signals::CONSOLE_SIGNALS`: the two directions of the data, and the hold |

A console has no lines to poll and no driver keeping a queue. `HOLD` is the one
control of that row it does have: `Session::set_read_hold` answers both kinds —
a port through `PortSupervisor::set_read_hold`, a console through
`PtySession::set_read_hold`, which is a `zyt_pty::ReadHold` the reading thread
waits on. Nothing is lost by either, so the two are one state with one letter
and one command (`port.toggle_hold`).

A console keeps its samples in the session and not in a worker:
`Session::sample_console` pushes one `zyt_serial::LineSample` per step of
`lines_interval`, the step a port is polled at, so the span under the tracks
means the same for both. Steps the window drew no frame for are filled with
samples of nothing crossing, since a byte crossing is what asks for a frame; the
fill is capped at `LINE_HISTORY_SAMPLES`.

**For a port the sampling is in the worker, not here.** `Session::pump`
refreshes `self.lines` once a frame, and a hidden window draws no frames — so
a sampler on the ui thread would read stale levels exactly when something is
happening, and would write nothing at all while the window is not drawn. The worker samples beside
`poll_lines`, where the step already *is* `lines_interval` and where the thread
knows both the levels and whether a byte crossed.

### Drawing it

`ui::signals::plate` draws an `Area` of `ui::plates::ORDER`, edge to edge of the
terminal and `ui::PLATE_GAP` from the edge it rises against — the one gap every
plate of the window keeps. The foot is `ui.max_rect().top()` of the status bar
and the head is `content_rect().top()`, no panel standing above the central one.

### Plates that stand together

`ui::plates::Plates` is what the plates drawn in this frame took, each with the
edge it stands against. It is cleared at the start of `ui::draw`. A plate drawn
later asks `Plates::free` where to stand, and stands `PLATE_GAP` past every plate
at the same edge that it would cross; one in the other corner is not in the way.
A plate is placed before it is laid out, so `plates::span` takes its width from
the frame before.

| plate | drawn | stands |
| --- | --- | --- |
| the signals | first, from the status bar | at its own edge |
| the times | after the signals, `statusbar::times` | above the status bar, or above the signals while they stand at the foot |
| the selection | from the terminal | in its corner, or past the signals or the times at that edge, on the cell grid still (`selection::on_grid`) |

```
 TX   ░░███░░░░░░░░░░░░░░░░░░░
 BRK  ░░░░░░███░░░░░░░░░░░░░░░
 HOLD ░░░░░░░░░░░░░░████████░░
 RTS  ██████████████░░░░░░░░██
 DTR  ░░░░████████████████████
      ────────────────────────
 RX   ░█░░░█░█░░░░░░░░░░░░░░░█
 CTS  ██████░░░░░░░░██████████
 DSR  ████████████████████████
 DCD  ░░░░░░░░░░░░░░░░░░░░░░░░
 RI   ░░░░░░░░░░░░░░░░░░░░░░░░
      ⏴ 1:40
```

The `HOLD` stretch above is what the plate is for: the reading was stopped, the
driver dropped `RTS` for exactly as long, and `RX` went empty. Three rows saying
one thing, which no row of letters showing what is true now could.

Two groups with a line between them, what this side drives above what the peer
does, each led by the data of its own direction because the handshake is what
leads to the bytes. The order is `ui::signals::TRACKS` and not the crate's:
`zyt-serial` knows which signals there are and which side drives each, and a
window knows the order they are read in.

Which rows stand is `PortMemory::shown_lines`, a `config::ShownLines` of the
device, and `statusbar::modem_lines` draws its letters from the same set, so one
switch decides both and they cannot disagree. `config::StatusLine` is the list
of what can be switched and the one place a letter is tied to a
`zyt_serial::Signal` and to the sentence the settings page names it by
(`StatusLine::note_key`). `ShownLines::draws` answers true for a signal no
letter names, which is how `TX` and `RX` are always drawn: they are what the
tracks of the lines are read against.

| | |
| --- | --- |
| a bar | one physical pixel, `1 / pixels_per_point` (`ui::signals::bar_width`), which is the most history a track can hold |
| the right edge | brought to a whole bar by `oldest_bar` first, because an edge that began between two pixels would smear every bar of every row |
| the bars | merged runs (`ui::signals::runs`), so a line that stood still for a screen is one shape and a single sample is one pixel wide |
| the two queue rows, `TXQ` and `RXQ` | a height instead of filled or empty: the share of `LineScale`, the fullest that queue has been seen |
| the layout | two columns, the names as wide as the widest of them and the tracks taking the rest |
| the span | written in the second column at `ui::signals::oldest_bar`, where the leftmost bar begins, which is not the left edge while the history is filling; a number in the corner of a plate would name nothing |

Four colours for four kinds of claim (`ui::signals::fill_color`), so a row is
read before its name is:

| colour | what it marks |
| --- | --- |
| green | a line standing up |
| red | the two things this side does that stop the line carrying, `BRK` and `HOLD` |
| warm orange, yellow | the two directions of the data |

They are written out rather than taken from the toolkit because they have to
mean the same thing in both colour modes, which is what `statusbar::level_color`
already does.

One consequence of the driver: `rts_up()` / `dtr_up()` fall back to what was
asked for where a line cannot be read back, and nothing here names that case, so
a line the driver does not report and a line that is down draw the same bar. The
crate keeps the `Option`; only what the window shows collapses the two.

While the plate stands it asks for a frame per `lines_interval`, and nothing
asks while it is down.
