# Interface

## Main area

Three views replace each other; none is a separate window, and each gives the
keyboard its own binding context.

```
┌──────────────────────────────────────┐
│ terminal | settings | file dialog    │  main area
├──────────────────────────────────────┤
│ status bar, or the search bar        │
└──────────────────────────────────────┘
   menu of plates and the plate of the times float above
```

The terminal is shown whenever the other two are not, connected or not: a window
whose session ended still holds what was on the screen.

The status bar carries controls only, never a message. Left to right: the
connection label, the signs of trust, mouse grab and a transfer holding the
stream, then pending output, running tasks, command history, search, gear. In
the settings it carries the way back and the gear alone: everything else names a
connection the settings may not be showing.

The search bar replaces the status bar and is shown even when that one is
switched off. It owns the keyboard while it stands (`ui.focus` is never
`Terminal` then), takes the terminal selection as its initial query, and closes
on a click in the terminal. Its four query kinds are a menu of plates and are
kept in the settings; `zyt-term` holds the pattern.

The file dialog is `egui-file-dialog`, sized to the main area each frame,
without a title bar, keeping a hundredth of the window free along each edge.
What it remembers is `<config>/file-dialog.yaml`, read on every opening and
written when a path is picked, so two copies of the program do not overwrite
each other.

Pages scroll with `widgets::scroll_area`, which takes the scrollbar out of the
input while a finger is down: a press on the track is a jump, and a swipe that
began on the bar threw the page wherever the finger went.

### Picking a file

`App::pending_pick` says what the dialog is picking for, and `take_picked_file`
is where each answer goes. `PendingPick::SaveSelection { open_with }` is the
selection of the terminal:

| step | |
| --- | --- |
| the entry is chosen | `App::save_selection` takes the text at once, because the dialog stands in place of the terminal and a program writing meanwhile is a selection that moved |
| the text waits | in `App::saving`, offered under `SELECTION_FILE` |
| a path is picked | a `zyt_files::FileTask::Write`, so a whole scrollback neither holds the window still nor lands without a way to stop it |
| nothing is picked | the text is dropped |

`Done::Written` answers with the path alone, so `App::saved_selections` holds
the number of that write and what to do when it lands: the clipboard takes the
path while `Settings::copy_saved_path` says so, and `App::open_file_with` takes
it when the entry chosen was the one that opens it. That second one asks nothing
of `refuses_path`, which is about an untrusted session's word for a file of this
machine; this is a file the window has just written where somebody picked. A
write the map does not name is some other write.

Nothing is said in the terminal about a save: a plate would cover what was worth
saving, and the path in the clipboard is what anybody does something with next.

## Selection

The gestures, the units and the anchor are the widget's and the emulation's:
`crates/zyt-term-egui/README.md` and `crates/zyt-term/README.md`. What the
application adds is the plate and what the view follows.

While a selection stands, `ui::selection` draws a plate of what it covers:
columns, lines and characters, counted by `Terminal::selection_size` over the
text the selection would copy, so the plate and the clipboard cannot disagree.

The plate takes the corner the **moving end** of the selection is furthest
from — `Terminal::selection_edge`, which is the pointer of a drag and the caret
of the keys — and both the corner and its width fall on the cell grid, so it
covers whole characters.

It is that end and not the pointer: a selection made with the keyboard has no
pointer in it at all, and the pointer of a drag that is over walks off to a menu
or another window while nothing about the selection changes. The end that moved
last moves only when the selection does. While it is off the page,
`UiState::selection_corner` is the corner the plate keeps.

## The selection as a mode

Picking out a selection is a state of the terminal (`Terminal::selecting`),
entered by the first press of a drag or the first step of the keys and left in
one way — `App::leave_selection` — however it was asked for:

| while it stands | |
| --- | --- |
| the pointer | belongs to the selection: `mouse_reports` is off whatever the program on the line asked for, because a drag reported to the program picks out nothing |
| the keyboard | sends nothing to the device: the keys belong to the selection, and the key that lets it go is the first one the device gets again |
| the status bar | carries `icons::SELECTION` in the error colour of the theme, beside the sign of trust, and a press on it lets the selection go |
| the plate of the counts | stands for as long as the mode does; a selection of blank cells counts nothing and says so |
| the two ends | are marked whatever `Settings.show_selection_ends` says |

| what lets it go | |
| --- | --- |
| `Esc` | and that press does not reach the device |
| a press on the plate of the counts | |
| a press on the sign in the status bar | |
| a press in the terminal with nothing held | a press with a modifier is building a selection — `Shift` grows it, `Ctrl` picks out a block — so it is left alone |
| an arrow, `Home` or `End` without the modifiers | the key reaches the device as well |

## Selection with the keyboard

`ctrl+shift` and an arrow picks out a block: `Terminal::select_by_key` puts the
anchor at the cursor of the device on the first step — that is where somebody
reading the output is looking — and every later step moves the far end, so the
block runs from the cursor to the caret. The kind is always a block, because the
keys walk by cells and rows and a run of text has no column to walk.

| | |
| --- | --- |
| the caret | the moving end, kept as a place in the text against the history of the moment, so output arriving carries it with its line |
| letting the keys go | changes nothing: a release is not a press, and the selection stands |
| an arrow, `Home` or `End` without the modifiers | typing again: `forget_key_selection` takes the caret and the selection, and the device gets the key (`walks_the_grid`) |
| any other key | leaves the selection standing — a line typed beside a selection is a line typed beside it |
| `ctrl+shift+Home` / `End` | takes the caret to the first or the last cell of the row it stands on, and stays on that row |

The page follows the walking end when it leaves it, by as little as it takes
(`reveal`, which the search uses for the same reason). Nothing wraps: the two
ends of a row are the ends of that row and not of the terminal, because what a
step of a block is measured in is the row it is on.

A sideways step the caret cannot take — it is against the left or the right
edge — moves the anchor the other way instead, so a key held down against the
edge goes on widening the block. A row is a span the eye takes in at once, so
that block is one somebody is watching grow; the grid is as tall as the history,
and one that grew downwards when asked to go up would walk away from the rows
being read, so up and down stop at the oldest line and the newest. The two ends
of a row are places and not directions, so they stop as well.

The view follows the end of the output only while it already stands there.
Dragging the scrollbar to the bottom, scrolling there, or typing puts it back.
`Session::follows_output` is asked before application notices are printed too.

## Identifiers of the window

Every plate over the terminal is an `egui::Area`, and an area answers a press by
itself. Asking its answer to sense one again — `Response::interact` — registers
the same identifier a second time in the same frame, which the toolkit answers
by painting *Double use of widget ID* over the window. So a plate reads
`response.clicked()` as it stands.

`src/ui/mod.rs` has the net for that: it draws the whole interface without a
window, with `Context::run_ui` over a store in a temporary directory, and reads
the shapes back looking for the complaint. Nothing of a window, a chip or a
device is needed to find out that two widgets are standing in one place.

## A command of several lines

`ctrl+shift+z` opens `src/ui/block.rs`: a window that does not move, with a
title, a cross, a field of five rows that grows with what is written, and the
button that sends under it against its right edge.

| | |
| --- | --- |
| where the text lives | `UiState::block`, the window's own copy — not in the device and not in the history until it is sent |
| the keyboard | `App::handle_keyboard` dispatches nothing while the window stands, the same as behind the ask window, so the field has every key |
| `ctrl+enter`, the button | `App::send_block_input` |
| `esc`, the cross | close it and send nothing: a command half written is not one somebody meant to keep |

What is sent is every line closed with a carriage return and one added at the
end (`typed_block`), because that is how a device ends a line and a block whose
last line waited for a key is a block that did not run. The whole of it is one
entry of `history::List::Source` and not of the commands added by hand: this is
a command that ran here, and the one thing wanted of it later is to run it
again.

## The menu

One widget draws every menu: the terminal menu, the session menu, a link menu,
the command palette, the command history, the sources, and every list to choose
from — a list is a menu with the entry in use marked.

`plate-menu` knows `egui` and `nucleo-matcher` and nothing else. Entries are
plain data built in `src/ui/menu.rs`; a choice comes back as
`plate_menu::Chosen` — the identifier and the modifiers held on that frame.

| identifier | meaning |
| --- | --- |
| a command id | run it |
| `link.*` | act on the link under the pointer |
| `profile:<name>` | pick a transfer profile |
| `history:<command>` | type a command back, out of the file of this source |
| `added:<command>` | type one back out of the shared file |
| `history.forget:<entry>` | take that command out of its file |
| `source:<kind>:<name>` | open a source |
| `choice:<list>:<slot>:<value>` | a value of the settings |

Keys, searching and the hooks of an entry are the widget's own
(`crates/plate-menu/README.md`). What uses them here: a port carries the name of
the device plugged into it as `search` text, and the removal below a command of
the history and *At start* below a source are `searchable(false)`.

While a menu stands nothing behind it is reached. That is the application's
part: `ui::menu::hold_input` makes the menu layer the modal layer of the toolkit
and surrenders the keyboard of whatever held it, and `App::active_contexts`
answers `CONTEXT_PALETTE`, in which nothing is bound.

The menu always stands in the middle of the window and never follows the
pointer. It closes on `Esc`, on a choice and on a click beside it — except the
question about a lost source, which uses `Beside::Ignored`.

## Key bindings

`zyt-keymux` takes `KeyStroke` values, walks the active context stack and
answers `Command`, `Pending` or `Unhandled`. The application maps egui events to
strokes, runs the commands and sends the rest to the terminal as bytes. The
palette searches the registry with `nucleo-matcher`; titles are translated
before registration.

`ctrl+shift+tab` hands the keyboard from the terminal to the status bar. Keys
are not dispatched at all while a menu, a confirmation or the ask window stands.

The clipboard is the one place a key press does not arrive as a key. The toolkit
answers `ctrl+C`, `ctrl+X` and `ctrl+V` with `egui::Event::Copy`, `Cut` and
`Paste` and swallows the press, and it answers the `Copy`, `Cut` and `Paste`
keys of a keyboard that carries them with the very same events. What is held at
that moment is what tells the two apart (`clipboard_shortcut_held`):

| | what the event is read as |
| --- | --- |
| a modifier is held | the shortcut it was: `clipboard_shortcut` rebuilds the stroke, offers it to the bindings, and lets what nothing claims reach the device — `^C` is owed to a program on the line |
| nothing is held | the key it says it is: `clipboard_key` copies the selection or pastes the clipboard, and sends no byte at all |

*Cut* copies, because the grid is what a device printed and there is nothing to
take out of it. While the keyboard is not the terminal's, neither path does
anything: a field of the interface answers the event itself.

`ctrl+Insert` on Windows cannot be told from `ctrl+C`: the toolkit folds it into
the same event with the same modifier held, and no press arrives to say
otherwise.

## The settings page

Two halves (`SettingsTab`), because the questions are two.

| *General* | *Connection* |
| --- | --- |
| appearance, fonts, performance | the console's name, program, directory |
| what a program may ask for | or a port's offered speeds |
| the library of transfer profiles | the values this source answers |
| | which profiles it offers and uses |

The *Connection* picker holds the consoles and the port this window is on. A
console is a file somebody meant; a port is a device the system found, so the
one worth a page is the one on the line. The page follows the connection until
something else is picked, and a pick that names nothing falls back to that.

Name, program and directory are text with a pencil to the left, not open
fields: a page of fields looks like a form whether or not anything is being
changed. The pencil is drawn apart from the value, so the directory can put its
picker button between them. The keyboard leaving closes the editor; the file is
written when the writing ends and the value changed, never per letter.
`UiState.editing` holds which one is open and what it held then. The text shown
has `{name}` resolved; what is written is the line itself.

Under the values stands a row per asker — the console, each offered profile —
with the names it wants. A name with a row is struck through; a name without one
is the button that makes it. A profile asking for something the source has not
got carries a warning triangle and is *not* switched off, because whether it is
offered is the user's answer. It is left out of the menu that starts one: that
menu is opened to start something now.

Ticking every profile is written down as ticking none, so a profile shipped
later is offered rather than quietly left out. Transfer profiles are a list the
user edits as a list, so they live in `profiles.yaml` of their own, and shipped
ones are added the way shipped consoles are.

Two sections fold under the sequence they belong to: the `file://` menus under
OSC 8, and the size of the command history under OSC 133.

## The window of numbers

`Settings.show_debug_window` opens it: processor share, memory now and peak,
threads with their names, frames, scrollback fill and cost, the buffers
(clipboard, pending output, read buffer, bytes in and out, last busy period),
fonts and atlas. Only the cross can be pressed. It scrolls past seven tenths of
the window height.

`src/metrics.rs` reads `/proc/self` twice a second, not per frame: a number that
moves every frame cannot be read, and the processor share needs two readings to
exist. Other platforms report `None` and the window shows a dash.

The *Render* block holds the last frame time — the work of one pass, without
what the toolkit spends afterwards — and the frames counted in the last second.
A rate derived from one frame time is a rate the program never drew at.
