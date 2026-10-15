# Settings without a page

Some settings live in the configuration file and nowhere else. Each is a figure
nobody looks for until something it governs goes wrong, and a control for it
would be one more row on a page read for other things.

They are keys of `config::SETTINGS_FILE` in the configuration directory
(`RULE config.dir`). A key the file does not carry reads as its default
(`RULE config.default`). The program writes the file back with every key in it,
so a key is found there to be edited once the program has run.

| key | unit | default | subject |
| --- | --- | --- | --- |
| `resize_delay` | milliseconds | `config::DEFAULT_RESIZE_DELAY`, held to `config::MAX_RESIZE_DELAY` | `docs/terminal.md`, *Resizing* |
| `screen_overflow.columns`, `screen_overflow.rows` | cells past the area | `config::DEFAULT_SCREEN_OVERFLOW_COLUMNS`, `config::DEFAULT_SCREEN_OVERFLOW_ROWS` | `docs/tmux.md`, *A window of another size* |
| `tmux.pause_after` | seconds | `config::DEFAULT_PAUSE_AFTER` | `docs/tmux.md`, *Flow control* |

## `resize_delay`

How long the terminal area has to stand still before the grid takes its size
and the program is told it. Zero takes every size at once, the way it was
before the delay existed.

A longer delay leaves the page drawn at its old size for longer after a window
is let go of. A shorter one lets a window being dragged slowly reach the
program as several sizes, each a redraw of a prompt or a menu that can leave
its last one behind.

## `screen_overflow`

How far the screen of a pane of tmux in control mode may reach past the area,
columns and rows apart. Until control mode the area was the only thing that
sized a terminal; the size of a pane comes from the other end of the line, and
tmux allows ten thousand cells each way. What is past the margin is cut, and
tmux is not told.

A margin rather than a figure, because what is worth keeping past the edge is
what a larger window would show next: a window of tmux a little larger than the
area stays whole, and a screen thousands of cells past anything that can be
shown is not paid for. A larger margin keeps more of a large window of another
client and costs its columns times its rows; `0` holds a pane to the area
exactly.

A line longer than the limit wraps here where tmux did not wrap it, so what
follows it stands a row lower than in tmux until the screen is drawn again; a
program that moves the cursor below the limit writes on the last row.

## `tmux.pause_after`

How far a pane of tmux may fall behind before tmux pauses it, read while the
switch `tmux.pause` of the page is on; tmux 3.2 and later. The line under the
switch names it, so the page says what is in force without a control for it.

Shorter skips sooner and loses more of a flood; longer lets the screen stay
further behind before anything is thrown away. A paused pane loses what piled
up for it, so the figure is how much lag somebody would rather skip than wait
through, and it is not a figure anybody looks for until a flood.
