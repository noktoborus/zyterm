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
connection label, the button of tmux while tmux control mode is on the line, the
signs of trust, mouse grab and a transfer holding the
stream, then pending output, running tasks, command history, search, gear, the plate of
a command of several lines. The line of a port is the one its driver has, in the warning
colour when that is not the line asked for (`docs/ports.md`, *The line the
driver has*). In the settings it carries the way back and the gear
alone: everything else names a connection the settings may not be showing.

The search bar replaces the status bar and is shown even when that one is
switched off. It owns the keyboard while it stands (`ui.focus` is never
`Terminal` then), takes the terminal selection as its initial query, and closes
on a click in the terminal. Its four query kinds are a menu of plates and are
kept in the settings; `zyt-term` holds the pattern.

`Enter` in the field walks the matches upwards and `shift+Enter` downwards,
which is the pair of buttons beside it and what their hints name. Upwards is
the way a search in a terminal goes: what is looked for has scrolled off the
top. The field surrenders the keyboard on either, so the bar takes it back on
the same frame and the next press walks on.

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
is where each answer goes. `PendingPick::SaveFile { open_with }` is one of the
two things the window holds and can write — the selection of the terminal, the
payload of a picture a program sent:

| step | |
| --- | --- |
| the entry is chosen | `App::save_selection` or `App::save_picture` takes the bytes at once, because the dialog stands in place of the terminal: a program writing meanwhile is a selection that moved, and a picture is dropped as soon as the budget needs its memory |
| the bytes wait | in `App::saving`, offered under `SELECTION_FILE` or `PICTURE_FILE` |
| a path is picked | a `zyt_files::FileTask::Write`, so a whole scrollback neither holds the window still nor lands without a way to stop it |
| nothing is picked | the bytes are dropped |

Both entries carry the same two labels — `save.to_file` and `save.open_with` —
because they are the same two acts; what they are about is said by the hint and
by the menu they stand in.

`Done::Written` answers with the path alone, so `App::saved_files` holds
the number of that write and what to do when it lands: the clipboard takes the
path while `Settings::copy_saved_path` says so, and `App::open_file_with` takes
it when the entry chosen was the one that opens it. That second one asks nothing
of `refuses_path`, which is about a session's word for a file of this machine
while its local links are off; this is a file the window has just written where somebody picked. A
write the map does not name is some other write.

Nothing is said in the terminal about a save. The desktop is told instead
(`App::told`): the path the file landed under and what it came to, which is the
answer to a save and is read, if it is read at all, beside whatever the person
is doing by then. A line in the terminal would push the output of the device up
instead, and the path in the clipboard is what anybody does something with next.

`PendingPick::FormPath` is one row of the window a script asks with, by the
name of that row. The dialog is a view of the whole window and the form is a
window over it, so `form::draw` returns at once while that view stands: the form
waits in `ui.form` with everything answered so far, `App::set_form_path` writes
the path into the row, and the window comes back with it. That is what lets a
script ask for a path among its other questions.

`PendingPick::Script` is a dialog a running script asked for with
`zyt.ui.pick`: the question waits in `App::picking` while the view stands, and
what is picked — or nothing, when the dialog is closed — goes back to the
script, which has been waiting on it. The dialog opens where that script
picked last time on that source (`SourceMemory::pick_directories`). A run that
ends while it stands is told nothing and the view is left. The plate of the
run is not drawn meanwhile, as it is not while a form stands.

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
covers whole characters. A plate of the signals or of the times at that edge
moves it past them (`ui::plates`, docs/signals.md).

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
| the keyboard | reaches the device as it always did, and what it sends lets the selection go: somebody who sends something is done looking at it |
| the status bar | carries `icons::SELECTION` in the error colour of the theme, beside the sign of trust, and a press on it lets the selection go |
| the plate of the counts | stands for as long as the mode does; a selection of blank cells counts nothing and says so |
| the two ends | are marked whatever `Settings.show_selection_ends` says |
| the source | is not read: `Session::holds_for_selection`, because a page that moves under a selection is the picking going wrong |

| what lets it go | |
| --- | --- |
| input sent to the device | `App::send_input`: keys, pastes of every kind, a dropped file typed in, a command from the history, the plate of several lines |
| `Esc` | and that press alone does not reach the device |
| a press on the plate of the counts | |
| a press on the sign in the status bar | |
| a press in the terminal that selects nothing | the widget puts the selection away; `Shift` and a press grow it instead |

Input is the rule, and a press is not: what ends the selection is bytes going
out. A press that sends nothing leaves it standing — a modifier on its own, a
key a binding claims, a key while a transfer holds the line. A binding is a
command, and most of the commands there are to run over a selection are about
the one standing. A middle button paste reads the selection before it sends it,
so pasting the selection is what lets it go.

What the terminal answers a program and what a script writes are not input,
and they end nothing.

What arrived while a selection stood is read as soon as it goes, in the order it
arrived: the hold is a reading left until later and nothing is thrown away by
it. Where the bytes gather meanwhile is what the source is — the same three
answers the `HOLD` switch of the status bar gives, and that switch is lit by a
selection as well, because output that stopped is a thing a person looks for an
answer to. What bounds the gathering is `Settings.read_buffer`, so a selection
left standing holds a device the way a window that cannot keep up holds it, and
no further. A script holding the line is never held: its protocol answers a
block and waits for the next one, and what it carries reaches no page.

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
| any other key | lets it go if it sends anything, as input does in every selection |
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

## What the window says, and where

| what it is | where it goes |
| --- | --- |
| something is true of the window itself — a theme file that will not read, a page put back to its defaults, a file dropped with nothing connected | `App::notice`, a notification of the desktop |
| a failure with a cause | `App::report`, a notification of the desktop: the message the heading, the chain of causes the body |
| what an act came to — a copy, a move, a removal, a file written, a program that ended, a console restarted, a link that would not open | `App::told`, a notification of the desktop |
| what a run of a script came to when it did not do its work — cancelled, failed with what it said | `Session::take_told`, said by the window through `App::told_in_full`. A run that started or did its work is told by nothing on the desktop: its plate standing and going away says it, and its end flags the window for attention the way a bell does (`Session::run_ended`, `App::handle_bell`) |
| what a script running prints | `Session::print_line`, a line in the terminal marked `│` |

The terminal is where the output of the device stands, so nothing of this
program is printed into it but the lines of a script running, which belong
beside the bytes that script moves. A block of the program would push the
output of the device up, stand in its history as if the device had said it, and
under tmux in control mode land in a pane tmux knows nothing of. What an act
came to is said in full — the path, how much went, the type it went as —
because that is what there is to know about it afterwards.

The same failure said again at once is said once (`App::last_notice`): a port
that fails every frame is one notification, and `App::notice` lets the next
one through.

`App::told` and the notifications a program asks for with `OSC 9` go out through
one call (`notify_desktop`), so a notification of this program is one of them
whoever asked for it.

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

`ctrl+shift+enter` opens `src/ui/block.rs`, and so does the button right of the
gear in the status bar: a plate against one edge of the terminal, edge to edge
of it, with a field of five rows that grows with what is written and a row of
controls under it against its right edge.

| | |
| --- | --- |
| where the text lives | `UiState::block`, the plate's own copy — not in the device and not in the history until it is sent |
| the keyboard | `App::handle_keyboard` dispatches nothing while the plate stands, the same as behind the ask window, so the field has every key; the field is asked for it again on every frame it does not hold it |
| `ctrl+enter`, the button that sends | `App::send_block_input` |
| `esc` | close it and send nothing: a command half written is not one somebody meant to keep |
| `ctrl+shift+enter` while it stands | nothing. The toolkit matches `ctrl+enter` against a press that also holds `Shift`, so `ui::block::sends` takes that press out of the events first: the key that opens a plate must not send what is in one |
| which edge it stands against | `UiState::block_at_top`, moved by the button at the left end of the row and kept for as long as the window runs |
| what it stands over | every other plate: they are drawn in `ui::plates::ORDER`, one layer below its `Foreground`, so a plate raised while a command is written cannot cover it. The menus and the windows that ask share its layer and stand over it when they open after it |
| the carets read as control codes | the switch left of the one that sends. `UiState::block_caret` is where it stands while the plate does, and `SourceMemory::block_caret` is where it is kept: it is of the source and not of the window — docs/sources.md |
| the first frame | `UiState::block_drawn`: the keys are read in `logic` and the plate is drawn in `ui`, so the press that opened it is still in the events when it first stands, and a plate that answered keys on that frame would answer that one |

The plate is drawn from `ui::draw` with what is left of the window once the
status bar has taken its own, so the one at the foot stands over the output and
never over the bar. There is one button for the move and not two: a plate at an
edge has one move, and the arrow on the button is the way it would go.

What is sent is every line closed with a carriage return (`typed_block`),
because that is how a device ends a line and not with a line feed. The breaks
of the field become returns before the carets are read, so a `^J` written by
hand is the line feed it stands for and not a break read a second time.

What closes the last line is the switch of the carets:

| the carets | the last line |
| --- | --- |
| characters | a return is added here, unless the field ends in a break: a block whose last line waited for a key is a block that did not run |
| control codes | what is written there closes it, and nothing else: a `^M` or a break of the field is that key, and a block carrying neither ends where the text does |

With them read, every control code of the block is one somebody wrote, so a
return added after a bootloader fed a key or a shell handed the end of its input
would be a line nobody typed. The whole of it is one
entry of `history::List::Source` and not of the commands added by hand: this is
a command that ran here, and the one thing wanted of it later is to run it
again. The switch goes into that entry as `Entry::caret` — docs/sources.md — so
the plate of the command says the carets of it were bytes. What a change to this
plate has to be read against in the history is in ARCHITECTURE.md.

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
| `script:<name>` | pick the script a transfer runs with |
| `form:<field>:<value>` | answer one list of a dialog a script is asking |
| `history:<command>` | type a command back, out of the file of this source |
| `added:<command>` | type one back out of the shared file |
| `history.forget:<entry>` | take that command out of its file |
| `source:<kind>:<name>` | open a source |
| `choice:<list>:<slot>:<value>` | a value of the settings |

Keys, searching and the hooks of an entry are the widget's own
(`crates/plate-menu/README.md`). The widget has no text of its own, so
`ui::menu::draw` hands it two on every frame, because the language can change
while the window runs: `menu.nothing_found`, the line of a query that finds
nothing, and `menu.step_in`, the name a screen reader gives the zone `›` of an
entry that is chosen and has entries of its own. What uses them here: a port carries the name of
the device plugged into it as `search` text, and the removal below a command of
the history and *At start* below a source are `searchable(false)`.

While a menu stands nothing behind it is reached. That is the application's
part: `ui::menu::hold_input` makes the menu layer the modal layer of the toolkit
and surrenders the keyboard of whatever held it, and `App::active_contexts`
answers `CONTEXT_PALETTE`, in which nothing is bound.

A list of a row of a dialog is opened once that dialog is standing again.
`ui::form::draw` takes the window out of `UiState` to draw it and puts it back
afterwards, so a menu built while the rows are being drawn would be built from
nothing and never came up at all: the row writes down that its list was asked
for, and the menu is opened after the state is back.

A transfer that is running changes what the terminal menu and the session menu
offer. The two directions and the plate of the scripts are not there — none of
them can be acted on while a script holds the line — and stopping it stands
where they were.

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

`Shift` with `Enter` has no code of its own, so what it sends is a setting
(`Settings.shift_enter`, `zyt_term::ShiftEnter`): the carriage return `Enter`
sends and nothing more, `\x1b\r` — what `Alt` with a key sends, and the
spelling most programs already read — or `CSI 13;2u` of the `CSI u` keyboard
protocol, which names the key and what was held with it. It is the
reader's answer because no program is asked: a prompt that takes several lines
reads one of them, and which one is not written down anywhere. Nothing else
about the key changes — held with anything else, it is a carriage return.

## The settings page

Two halves (`SettingsTab`), because the questions are two.

| *General* | *Connection* |
| --- | --- |
| appearance, the keyboard, fonts, performance | the console's name, program, directory, environment |
| what a program may ask for | or a port's offered speeds |
| the scripts that were found | the values this source answers |
| whether a script asks every time | which scripts it offers and uses |

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

Under the values stands a row per asker — the console, each offered script —
with the names it wants. A name with a row is struck through; a name without one
is the button that makes it. A script asking for something the source has not
got carries a warning triangle and is *not* switched off, because whether it is
offered is the user's answer. It is left out of the menu that starts one: that
menu is opened to start something now.

Ticking every script is written down as ticking none, so a script installed
later is offered rather than quietly left out. A script is a directory and not
a setting, so the *General* half has nothing to edit about one: it names what
was found — the name a person reads, the name of its directory, which ways it
carries a file — gives each row the button that opens that directory, and says
underneath what could not be read as a script at all. Beside them stand the two
things somebody writing one wants: read the directories again, and open the
directory of one's own. `docs/scripts.md` is the subject.

The third button is *Reset form saving*. A form told not to ask again stops
coming up, and a window that does not come up has nothing in it to press, so
the way back is here: every form that was told so is put back to asking, and
how many were changed is said in the terminal.

Two sections fold under the sequence they belong to: the `file://` menus under
OSC 8, and the size of the command history under OSC 133.

The block *tmux* follows the sequences (`ui::settings::tmux`). Its rows are
switches, and under each, in place of a hint, a line that says what the switch
does in the position it stands in: `tmux.history`, history loaded by
scrolling or on demand (`docs/tmux.md`, *History on demand*);
`tmux.pause`, with `tmux.pause_after` in its line (*Flow control*); and
`tmux.focus_follows_mouse`, the focus moved by the pointer or by a press
(*Panes side by side*). The shade of the panes out of focus, `tmux.shade`, is
a slider with a line under it that does not change.

A few settings are on no page at all and are edited in the configuration file:
`docs/config-only.md`.

## The button of tmux

It stands right of the connection label while tmux control mode is on the line
(`statusbar::tmux_button`), and says what speaks on that line now; the label
names the line, which is the same with tmux on it or not.

| state | look | hint | pressed |
| --- | --- | --- | --- |
| entering | `⊞ ⋯ 12`: the sign, the dots of waiting, the bytes typed and held | `status.tmux_entering_hint` | gives up: what came is shown as text, what was typed is sent |
| standing | `🗖 1: vim`: the window shown; `⊟ 1.1 bash`, the pane shown of a window of several; `⊞` before the window is known; `  🔔2` after it, in the colour of the selection, while windows call | `status.tmux_calling_hint` while windows call | the menu of tmux (`docs/tmux.md`) |
| leaving | `⊞ ⋯ 12` | `status.tmux_leaving_hint` | the plain terminal at once |

`RX −14 s` stands beside `TX` while the screen of a pane of tmux is a second or
more behind and `tmux.pause` is on: grey while nothing is lost, red once output
was thrown away; pressed, what piled up is skipped (`docs/tmux.md`, *Flow
control*).

`icons::TMUX` is U+229E, a square cut in four as tmux shows its panes.
`emoji-icon-font` carries it in both families, so a button draws it;
`◫` and `▦` say the same and only Hack, in the monospace family, carries them.
Standing, the button wears the icon the menu gives what it names:
`icons::WINDOW` or `icons::PANE` (`Control::named_pane`).

## The debug window

`Settings.show_debug_window` opens it: processor share, memory now and peak,
threads with their names, frames, scrollback fill and cost, the buffers
(clipboard, pending output, read buffer, pictures kept against their budget,
bytes in and out, last busy period), fonts and atlas. The pictures are there
because they are the one thing a device can make this program hold megabytes of
without anybody asking it to: the row says how many are kept, what they take
and what they may take (`Terminal::pictures_held`). The scrollback and the
pictures are summed over every terminal of the window — the plain one and, under
tmux control mode, each pane with one — since the window reports the memory of
the window. A section *tmux* stands while the mode does: its version and state,
the session, the size of the window shown, the pane shown and its size, the
panes with a terminal, the commands awaiting their reply, the input held. Only
the cross can be pressed. It scrolls past seven tenths of
the window height.

`src/metrics.rs` reads `/proc/self` twice a second, not per frame: a number that
moves every frame cannot be read, and the processor share needs two readings to
exist. Other platforms report `None` and the window shows a dash.

The *Render* block holds the last frame time — the work of one pass, without
what the toolkit spends afterwards — and the frames counted in the last second.
A rate derived from one frame time is a rate the program never drew at.

## What a screen reader reads

eframe is built with `accesskit`. The tree egui builds of the widgets goes to
the accessibility interface of the system: AT-SPI on Linux, UI Automation on
Windows. A screen reader, voice control and an inspector read the window
through it; the window tests read the same tree (`RULE test.window`).

| part | what accesskit is given |
| --- | --- |
| the window | its title (`access::name_window`, every pass) |
| a button, a field, a slider of egui | role, value, state; the name is the text on it |
| a button of an icon | its hint as the name (`Named::tip`) |
| a switch (`widgets::switch`) | a checkbox, named by its row |
| a value beside its label | the label as the name (`Named::named`) |
| a plate of the menu | a button named by its label, selected, disabled; an entry that is chosen and has entries of its own is a second button, its zone `›`, named `menu.step_in` (`crates/plate-menu`) |
| the terminal | a node with no name and no text |

The terminal gives nothing of what it shows yet; an accessible terminal is a
goal of its own (README).

The tree is built only while something asks for it. On Linux `accesskit_unix`
joins the accessibility bus when `org.a11y.Status.ScreenReaderEnabled` is true,
not `IsEnabled`: Orca sets it, an inspector may not.

To read the tree on a display (`RULE gui.display`) with no desktop around it,
`tools/atspi-tree` starts the bus of accessibility and its registry, says a
screen reader runs, starts the program and prints each node's role, name and
states; `--click` presses a node by its name first:

```
dbus-run-session -- tools/atspi-tree --click Settings -- target/debug/zyterm
```

It needs `at-spi2-core`, `dbus`, `python3-gi` and `gir1.2-atspi-2.0`. On a
desktop, Accerciser shows the same tree and presses a button by its action. On
Windows, Accessibility Insights or `Inspect.exe` read the tree of UI
Automation.
