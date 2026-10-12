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

## `resize_delay`

How long the terminal area has to stand still before the grid takes its size
and the program is told it. Zero takes every size at once, the way it was
before the delay existed.

A longer delay leaves the page drawn at its old size for longer after a window
is let go of. A shorter one lets a window being dragged slowly reach the
program as several sizes, each a redraw of a prompt or a menu that can leave
its last one behind.
