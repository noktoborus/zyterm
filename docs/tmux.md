# tmux control mode

`tmux -CC` is a client of tmux that draws nothing: it speaks a line protocol,
and this program shows the panes of tmux as terminals of its own.

```
plain tmux:   program ──► pane ──► tmux draws all panes as one screen ──► terminal
tmux -CC:     program ──► pane ──► tmux sends the bytes of the pane,
                                   tagged with its id ──► a terminal per pane
```

What it gives: a scrollback per pane that search, selection and the command
history work on; programs on the alternate screen that leave that scrollback
clean; sixel pictures reaching the emulator untouched; a session that outlives
the line, `tmux -CC attach` bringing it back.

The protocol is `crates/zyt-tmux/README.md`. This chapter is what the program
does with it. The panes of the window tmux shows are drawn side by side, as
its layout puts them; the keys go to one of them, the pane shown.

## Where it is found

`Session::pump` hands the bytes of the line to `Session::take_in`
(`src/session/mode.rs`), where `terminal.feed` was called before. With the switch
of *Terminal Features* off and no mode standing, that is `terminal.feed` and
nothing else.

```
spare ──► Splitter ──Plain──────► the plain terminal
                   ├─Entered────► Entering: held until the first line
                   ├─Control────► tmux::Control (src/tmux/) ──► a terminal per pane
                   └─Left───────► the end of the mode
```

Not in `osc.rs`: `vte` ends a DCS on `0x9C`, and tmux sends UTF-8 as it is, so
`ќ` (`D1 9C`) would end the mode. Not in the thread of the port or of the pty:
workers never look into the bytes, and a port and a console behave the same.

## The states

```
             ESC P1000p              first line is %begin … 0
  Plain ─────────────────► Entering ─────────────────────► Control
    ▲  first line another,    │                               │ %exit, detach,
    │  held past the buffer,  │                               │ the switch off
    │  or the button: given up│                               ▼
    └─────────────────────────┘                            Leaving
    ▲                                                         │
    └──────────── ESC \, the button, a disconnect ────────────┘
```

| state | bytes of the line | shown | written to the line |
| --- | --- | --- | --- |
| `Plain` | the plain terminal | the plain terminal | what is typed |
| `Entering` | held, up to the read buffer | the plain terminal | nothing; input held |
| `Control` | `tmux::Control` | the pane shown; the plain terminal until there is one | commands of tmux |
| `Leaving` | `tmux::Control`, until `ESC \` | the pane shown | nothing; input held |

`Entering` exists because `ESC P1000p` alone proves nothing: a device that sent
it by accident gets its bytes back on the plain terminal, without the sequence,
and nothing was written to it. Neither waiting state has a timer; a device gone
quiet, or a slow line with output queued before `%exit`, is the user's to judge
with the button of tmux.

| from | the input held |
| --- | --- |
| `Entering` → `Control` | dropped: it was for the shell, and the shell is gone |
| `Entering` given up | written to the line |
| `Leaving` → `Plain` | written to the line, for the shell that came back |

A disconnect by hand drops the mode with nothing written, and the next
connection starts on an empty plain terminal. A tmux still running at the other
end of a port then writes its protocol into that terminal as text.

## Entering

```
the opening pair %begin … 0 / %end      → display -p '#{version}'
the version, 3.0 or later               → refresh-client -C ; list-sessions ;
                                          list-windows -a ; list-panes -a ;
                                          new-session -t '$S' ;
                                          set destroy-unattached on
%session-changed of the new session     → the lists again; select-window of the
                                          window that was current in $S
the lists                               → the snapshot of the active pane of the
                                          window shown
the end of the snapshot                 → the pane shown, the plain terminal hidden
```

Nothing is sent before the opening pair ended: a start that fails — `no
sessions` — answers that pair with `%error`, and the client of tmux is gone by
the time a command would arrive. A tmux older than 3.0 is left with `detach`:
every byte typed goes through `send-keys -H`, which 3.0 brought.

## A pane

A pane gets a terminal of its own when it is first shown, and keeps it. A
snapshot is one line of commands, so tmux answers with no output between them:

```
display -p -t %N '<MODES>' ; capture-pane -p -e -J -a -q -t %N ; capture-pane -p -e -J -t %N
```

It is written into a terminal made anew at the size `display -p` gives:

1. the screen under the alternate one, when the alternate screen is on, then
   the cursor the program saved and `?1049h`, so `?1049l` brings that screen back;
2. the visible screen: `ESC[H`, the lines joined by `\r\n`, none after the last;
3. the modes `capture-pane` does not carry — the scroll region, wrapping,
   insert, the keys of the application mode, the cursor shown, the mouse modes,
   the tab stops — and the cursor.

`-J` joins wrapped lines, which wrap again at the width of the pane and are
copied as one line. `\r` clears a pending wrap, so a line of the full width is
not wrapped twice.

| state | its `%output` |
| --- | --- |
| none | its window named by the lists: a terminal made, `Unseen`, and fed; dropped otherwise |
| `Unseen` | fed: what a hidden pane says is read as it comes. Its screen holds only what came since, so shown it is shown as it is and snapshotted |
| `Loading` | fed to the terminal that stands until the snapshot ends, not to the one the snapshot is written into: the snapshot holds it |
| `Fresh` | fed, shown or not |

tmux sends the `%output` of every pane of the session this client is on, hidden
or not. A terminal is made only for a pane of a window the lists named, so their
limit (`windows::MAX_ENTRIES`) holds how many there are, not the line.

A snapshot whose reply was cut short is asked again, three times at most. A
pane closed in tmux — gone from its layout, its window closed — goes with its
terminal.

The answers of a pane's emulator are dropped: tmux already answered the
program. What the shown pane's events say — title, directory, progress, the
bell, the clipboard — is the session's; the commands a shell marked are kept
from every pane.

## Input and size

What is typed goes as `send-keys` to the pane shown: printable text by `-l --` in
single quotes — after `--`, since a piece that begins with `-` would be read as
a flag and lost — everything else by `-H` — control bytes, an escape sequence
whole, bytes that are not UTF-8. One `send-keys` is on its way at a time; what is
typed meanwhile goes with the next, so a slow line carries keys in fewer and
larger commands. A paste goes in pieces of `tmux::PIECE`, so a key typed during
it is not held behind all of it.

A key whose encoding depends on the terminal goes by its name in tmux instead:
`send-keys -t %N -- 'Home'` (`src/tmux/key.rs`). tmux then encodes it for the
pane, as it does for a client that is not in control mode. As bytes it would
reach the pane as this terminal encodes it, which is not what the terminfo of
the pane says.

| key | this terminal, cursor keys mode | tmux, to the pane | terminfo of `tmux-256color`, `screen` |
| --- | --- | --- | --- |
| Home | `ESC O H` | `ESC [ 1 ~` | `khome=\E[1~` |
| End | `ESC O F` | `ESC [ 4 ~` | `kend=\E[4~` |

vim reads `ESC O H` there as `ESC`, `O`, `H`: a line opened above with `H` in
it. The keys that go by name are the arrows, Home, End, Page Up and Down,
Insert, Delete and F1 to F12, with Ctrl, Alt and Shift as `C-`, `M-`, `S-`. Text,
control bytes, Enter, Tab, Backspace and Escape go as bytes: every terminal
encodes them alike. The held input (`src/tmux/input.rs`) keeps the bytes of a
key beside its name, since what is held when the mode ends goes to the line.

The view of a pane does not resize its grid (`TerminalView::auto_resize(false)`):
the size of the area goes to tmux as `refresh-client -C`, after the delay of a
resize, and `%layout-change` sizes the panes. A pane of another size than the
area is drawn from the corner, short of it or cut by it.

## The menu of tmux

The button of tmux opens it while the mode stands (`App::open_tmux_menu`,
`src/ui/tmux.rs`), on the window shown, so `Enter` at once changes nothing.

```
  tmux 3.3a · 120×40 · clients: 2      ← notice: version, size of the window, clients
  Session: main                        ← the session shown, while there are others
┌───────────────────────────────┐
│ 0: bash                     › │
│ 1: vim                 2    › │──► 1.0 vim      ›    ← a window of two panes: their count,
│                               │    1.1 bash   • ›──► Zoom          Right or its › to its panes,
│                               │    ──────────        Close «1.1 bash»   the splits and Rename
│                               │    Split vertically
│                               │    Split horizontally
│                               │    Rename…
│ 2: htop                   • › │──► Split vertically, Split horizontally, Rename…
│───────────────────────────────│
│ Session: logs          2    › │  ← another session: Enter goes there, Right or › to its windows
│───────────────────────────────│
│ New window                    │
│ Close window                › │──► Close «2: htop»
│ Detach from tmux              │
└───────────────────────────────┘
```

With one session the line `Session: main` and the block of other sessions are
not there: there is no other session to tell it from.

| entry | chosen |
| --- | --- |
| a window of the session shown | `select-window -t @W`, and `resize-pane -Z -t @W` when it is zoomed |
| a pane, one step in from a window of several | the pane shown and zoomed (below); `select-window` first in another window |
| *Zoom* / *Unzoom*, one step in from a pane | the pane zoomed, or unzoomed when its window is zoomed on it |
| *Close «1.1 bash»*, one step in from a pane | `kill-pane -t %P`; chosen twice, since it ends the program |
| *Split vertically* / *Split horizontally*, one step in from a window | `split-window -h` (the panes side by side) or `-v` (one above the other) of the pane it shows; the new pane, which tmux makes active, is shown |
| *Rename…*, one step in from a window of the session shown | a question of the new name, the name standing in it; `rename-window -t @W -- name`. An empty name or the same one sends nothing |
| another session | `new-session -t '$S' ; set destroy-unattached on`: a session of its group of its own (*A session of its own*), on the window current in `$S` |
| a window of another session | the same, then `select-window -t @W` in it |
| *New window* | `new-window -t $S` in the session shown |
| *Close window* | one step in, the window shown: `kill-window -t @W`. A window closed ends its programs, so it is chosen twice |
| *Detach from tmux* | `detach`; the mode is `Leaving`. It is the way out by hand: no key leaves the mode |

A pane is chosen here and not in tmux: no `select-pane`, since the active pane
is the window's and every client sees it change. A zoom does change it: tmux
makes the pane it zooms the active one, for every client, as a zoom in tmux
itself does. The choice stands while the
window holds the pane, until another is chosen; `%window-pane-changed` moves the
active pane, shown when nothing was chosen here, and does not undo the choice —
it does not say whether tmux changed the pane before the choice or after it,
and on a slow line one made before comes after. The button names a pane of a
window of several by the indexes and the command, `⊞ 1.1 bash`. A zoom made by
another client is followed through the visible layout of `%layout-change`: the
zoomed pane fills its window, the hidden one keeps its size, and nothing is
asked of tmux.

What a zoom sends goes by what the last `%layout-change` said: a window is
zoomed when one pane of several is seen (`Window::zoomed`).

| the window | a pane zoomed | a window unzoomed |
| --- | --- | --- |
| not zoomed | `resize-pane -Z -t %P` | nothing |
| zoomed on this pane | nothing | `resize-pane -Z -t @W` |
| zoomed on another | `select-pane -Z -t %P`; before 3.2, `resize-pane -Z -t @W ; resize-pane -Z -t %P`, which resizes the panes twice | `resize-pane -Z -t @W` |

`resize-pane -Z` toggles, and of a window zoomed it unzooms whatever pane it
names. `if -F '#{window_zoomed_flag}'` would decide on the side of tmux, but it
answers with one reply or two by the branch taken, and the client takes one
reply a command. Another client toggling the zoom in between turns the command
the other way; the next `%layout-change` shows it. The wheel sends no zoom: a
window it rests on keeps its zoom.

The question of the name is a window of its own (`ui::tmux::rename`): the menu
has no field to type in, and its query is the search. The new name comes back
as `%window-renamed`.

The menu opens on what is kept and asks the lists and the notice again in one
line; their replies refill it (`PlateMenu::refill`), keeping the query, the
level and the selection, and only while the menu standing is still the menu of
tmux (`PlateMenu::holds` of *Detach*, which every menu of tmux has). It never
waits for the line: on a port a reply takes as long as the line does. While the
notice is on its way, what tmux said last stands.

What the lists name is kept in `src/tmux/windows.rs`, and kept current by
`%window-add`, `%window-close`, `%window-renamed`, `%session-renamed`, the
`%unlinked-window-*` of other sessions and `%layout-change`; `%sessions-changed`
asks the lists again. What a listing no longer names is gone. No more than
`windows::MAX_ENTRIES` sessions and windows are kept, since the numbers come
from the other end of a line; the menu ends with a line of how many more. A name
longer than a plate is cut, whole on the plate beside the menu.

### The wheel over the button

The wheel over the button of tmux walks the windows of the session shown, in
the order of the menu: up the window before, down the window after, round past
either end. It walks windows, not panes: a window shows its pane as a choice
in the menu does. A session of one window does not move.

| while it turns | once it rests (`statusbar::TMUX_WHEEL_REST`) |
| --- | --- |
| each window shown at once, as it is, ahead of tmux; nothing sent | `select-window` of the window it rests on, and the snapshot of its pane when that has no screen |

So the windows walked past are not made current in tmux: they keep the bell
tmux holds for them, their zoom, and the window tmux calls the last one. A
step of a touchpad adds up to a notch (`statusbar::TMUX_WHEEL_STEP`) first.

### Windows calling

A window not shown calls when a bell rings in it or a program in it asks for a
notification (OSC 9, OSC 777). tmux sends the `%output` of every pane of the
session, so a hidden pane has a terminal and its events are read
(`State::pane_for_output`); tmux itself reads no OSC 9 or 777.

| from | what | kept by |
| --- | --- | --- |
| a bell or a notification of a pane not shown | the mark of its window, the last notification standing | `tmux::marks::Marks`, until the window is shown |
| `#{window_bell_flag}` of `list-windows -a` | a bell tmux saw, in this session or another | `Window::bell`, with every listing |
| `refresh-client -B 'zyterm-bells:@*:#{window_bell_flag}'` | the same flag as it changes, `%subscription-changed`; tmux 3.2 and later | `Window::bell` |

| where | what |
| --- | --- |
| a window in the menu | `🔔` a bell, `✉` a notification, before its count of panes or `•`; the order of the windows does not change |
| the plate beside the menu | the heading and the text of the last notification, and how long ago |
| `Session: X` | `🔔` when a window of it has the flag |
| the search of the menu | a window calling answers `tmux.calling_search` |
| the button of tmux | `🔔N`, the windows of the session calling, in the colour of the selection |
| the desktop | the bell asks for attention as a bell of the pane shown does (`App::handle_bell`); a notification goes to the desktop with the window named in its heading, `1: vim: built` |

A mark goes when its window is shown, or when the window closes; the flag of
tmux goes when tmux takes it down, which it does when the window becomes
current. `#{window_activity_flag}` is not asked: it needs `monitor-activity`,
an option of the session that this side does not turn on, and output alone is
not somebody calling.

## Panes side by side

Every pane seen of the window shown is drawn at once, a terminal and a view of
its own, where the layout of tmux puts it (`tmux::Places`, from the visible
layout of `%layout-change`). A window zoomed, one pane seen, is that pane
alone. The layout of tmux is the one that holds: this side asks to change it
and follows what tmux says.

```
┌───────────────────┬────────────────────┐
│ 0.0 vim           ┃ 0.1 bash           │   the pane shown: keys, cursor
│ ░░░░░░░░░░░░░░░░░ ┃ $ make             │   a pane beside it: shaded, no
│ ░░░░░░░░░░░░░░░░░ ┃                    │   cursor; a press gives it the keys
└───────────────────┴────────────────────┘
                    ┃ the divider: the cell tmux keeps, dragged
```

| what | how |
| --- | --- |
| the pane shown | the main view, in its place; it reports the size of the whole area (`TerminalView::report_area`), which goes to tmux as before |
| a pane beside it | a view of its own over a page the board carries for it (`Board::put_beside`), shaded by `tmux.shade` of the settings, no cursor |
| a press in a pane beside | the focus moves to it (`Choice::Focus`), here alone: no `select-pane`. The press then goes to its program when that reads the mouse |
| the pointer moved over a pane beside, `tmux.focus_follows_mouse` on | the focus moves to it, as by a press. Only a pointer that moves with no button held: a drag begun in the pane shown stays its own, and a pane chosen in the menu or by a key keeps the focus under a pointer at rest. Off, the default, only a press moves it |
| the wheel, a motion over a pane beside | to its program when that reads the mouse, by `send-keys -t %P` of that pane, the focus staying where it is (`Control::input_to`); its scrollback otherwise. It waits behind the keys of the pane shown, one `send-keys` on its way at a time, and what comes meanwhile goes joined; it does not leave with the mode as typing does |
| a divider | a thin line in the middle of the cell tmux keeps between two cells, dragged by the whole cell, a cell at a step. Let go, one `resize-pane -t %P -x <columns>` or `-y <rows>` for a pane of the cell before it; until `%layout-change` it stands where it was let go |
| a pane with no screen yet | shown as it is under the plate of *Switching*, over its own middle; every pane seen is snapshotted, the pane shown first |

A press, a key, a search, a selection, the history and `RX` are of the pane
shown. The button names it in a window of several: `⊞ 1.1 bash`.

## A session of its own

The current window of tmux is the session's, not the client's: a window
chosen by one client of a session is chosen for every client of it. So this
client goes to a session of its own, in the group of the one attached to:
a group shares its windows, and each session of it has a current window of
its own.

```
server ─┬─ session main     ($0) ─ current window @1 ◄── tmux attach elsewhere
        │       └── windows @0 @1 @2 ─┐  shared by the group
        └─ session main-1   ($1) ─ current window @2 ◄── this client
                └── the same windows ─┘
```

| when | sent |
| --- | --- |
| the first start, after the lists | `new-session -t '$S' ; set destroy-unattached on`: tmux makes the session, moves this client to it, and marks it to end once no client is on it |
| `%session-changed` of it | the lists again; then `select-window -t @W` of the window current in `$S`, since a session of a group starts on its first window |
| another session chosen in the menu | the same for that session; the session left ends, no client being on it |
| `detach`, the line lost for good, the program gone | tmux ends the session; the one attached to stays as it was |
| the port back after a loss | nothing: the client is on its session still |
| `new-session` refused | the client stays on the session attached to, and follows it |

What stays shared: the windows, their panes, the active pane and the zoom of a
window, and so the size of a window two clients look at. `list-windows -a`
lists a window once for each session of the group; the model keeps it with the
session of this client. The sessions of the group but this one have no window
of their own and are not in the menu; the session shown is named by the one
attached to. Another client sees the session of this one in its `tmux ls`.

## Switching

A window of the session shown, chosen here, is shown at once, ahead of tmux:
it is the window aimed at (`Model::shown_window`) until tmux says it is
current. Another session is switched to when tmux says so, by
`%session-changed`: its panes have no output here before.

The `%session-window-changed` of the session attached to, or of any other but
the session of this client, moves nothing: this client is on a session of its
own (*A session of its own*), and another client choosing a window there
chooses it in its session alone.

| while a window is aimed at | what is done |
| --- | --- |
| `%session-window-changed` of it | tmux reached it: it is the window shown, aimed at no more |
| `%session-window-changed` of another, a `select-window` on its way | what tmux did before; the window aimed at stays |
| the same, nothing on its way | somebody else chose it after: it is followed |
| `%error` of the `select-window` | given up: the window tmux says is current is shown |
| `%end` of it, and tmux says the window is current already | tmux says nothing more of a window that was current |
| the window closed, the session changed, the line lost | given up |

tmux 3.0a to 3.8 send `%session-window-changed` after the `%end` of the
`select-window`, and nothing for a window that was current.

| the pane of the window now shown | on the screen |
| --- | --- |
| has a fresh terminal | shown at once |
| `Unseen` | shown at once as it is, and snapshotted; its terminal stands, fed, until the snapshot is whole (`Pane::standing`) |
| has none | shown at once, empty, and snapshotted |

A pane shown without its screen is under the plate *Loading the screen…* over
its middle, from a delay on (`ui::terminal::LOADING_PLATE_DELAY`), so a
snapshot that comes at once does not flicker it. The plate stays while the
pane shown waits, one pane after another. Keys go to the pane at once, by
`send-keys -t %N`; the mouse reaches it only after the snapshot, which brings
the mouse modes of its program. The first pane of the mode waits for its
snapshot, the plain terminal standing until then. A pane of a session left loses its terminal once a pane of the
new session is shown: it gets no `%output` while the client is away, so shown
again it is rebuilt from a snapshot, and a terminal kept for it would hold only
memory.

`new-window` is answered by `%session-window-changed` and `%window-add` alone
when the size did not change: no layout and no pane is named. A window shown
whose pane nobody named has the lists asked, once for that window, and their
end shows its pane.

## A window of another size

`refresh-client -C` says how large this client is; how large a window is, tmux
decides by `window-size` across its clients. Another client attached — a plain
`tmux attach` elsewhere, another window of this program — can make a window
smaller or larger than the area.

From 3.1 `window-size` is `latest` by default: a window takes the size of the
client tmux calls the latest. `refresh-client -C` does not make a client that,
and keys sent by `send-keys` do not either; a `select-window` does. So when the
area changes, the window tmux has current is selected again in the same line:
`refresh-client -C 100x30 ; select-window -t @W`. The window resized last sizes
the window of tmux, whichever client attached last. Nothing is sent while the
area stands, so this side does not fight for the size. While the wheel turns,
the window selected is the one tmux has, not the one the wheel is on. 3.0a has
no `latest` and sizes a window by the smallest client; it is told the size
alone.

One size is asked at a time. Each one makes tmux resize the panes, and their
programs draw their screens again, which comes back as `%output`. A window
resized many times a second on a slow line, or through ConPTY, asked for more
than the line carried, and what was typed waited behind all of it. While a
`refresh-client -C` has no answer, the area is only kept; when the answer
comes, the size the area has then is sent, if it changed. The sizes between
are left out, and the last one always goes.

| the window of tmux | drawn |
| --- | --- |
| smaller | from the top left corner; the rest of the area hatched with diagonal lines in the foreground of the terminal, from the corner of the area and not along the rows of the grid, so no line marks its edge. What is short of a whole cell is not hatched: the plain terminal leaves it too |
| larger | cut on the right and at the bottom, a red wavy line along each edge that cuts |

The widget reports where the grid stands and which sides are cut
(`TerminalOutput::grid`, `cut_right`, `cut_bottom`); the binary draws the rest
(`ui::terminal::margins`).

A pane is held to the area and a margin past it, `screen_overflow`
(`docs/config-only.md`), each side apart: the size of a pane comes from the
other end of the line, tmux allows 10000x10000, and a screen costs its columns
times its rows whatever the scrollback budget says. What is past the limit is
cut and tmux is not told. The limit moves with the area: a window of this
program made larger gives a pane back what was cut.

## Flow control

tmux 3.2 and later can pause a pane that falls behind: `refresh-client -f
pause-after=N`, and a pane whose oldest unsent output is `N` seconds old is
paused, `%pause %P`, what was queued for it thrown away until `refresh-client -A
'%P:continue'`. Its output then comes as `%extended-output %P <age> : …`.

| | |
| --- | --- |
| `tmux.pause`, off — the default | nothing asked: tmux takes from a pane only as fast as its control clients take, and nothing is lost |
| on | `pause-after=tmux.pause_after` with the lists at the start, and at once when the switch moves while the mode stands (`Control::set_pause`); never before 3.2 |
| the age | of the last output of the pane shown, from `%extended-output` |
| `RX −14 s` | beside `TX`, from a second behind (`statusbar::received_lag`); grey while nothing is lost, red once the pane shown was paused, until the screen is back under a second |
| `RX` pressed | `refresh-client -A '%P:pause'`: what piled up is thrown away, and the `%pause` it brings lets the pane go as below |
| `%pause` of the pane shown | `continue` and a snapshot under a gap, at once |
| `%pause` of a hidden pane | left paused, its terminal as it stood; shown, `continue` and a snapshot under a gap |

A snapshot after a pause does not start the terminal over: the screen as it
stood goes up into the scrollback, the line of a gap under it
(`Terminal::mark_gap`, `crates/zyt-term/README.md`) says how much was skipped —
`14 s skipped, in the history of tmux` — and the snapshot is written below
(`Pane::begin_gap`). The modes a snapshot only turns on are turned off first. A
pane on the alternate screen, which has no scrollback, starts over as any
snapshot does.

What was seen of the age, with tmux behind `pty-throttle` at 9600: it stays
under a few milliseconds. tmux ages what it has not sent to its client, and the
client of tmux on the far end of the line takes everything the server sends and
holds it itself — with control clients alone, with a plain client attached
beside them, and with this window holding the line for a selection. tmux 3.8
paused a pane whose client was not read at all for four seconds; 3.3a did not.
So `RX` and the gap answer a client that stops taking, not a slow line as such;
the slow line is held by the pressure, and loses nothing.

A selection in the pane shown holds the line, as it does in the plain terminal
(`Session::holds_for_selection`).

## History on demand

A snapshot takes the screen and no history. What tmux keeps above it comes a
screen at a time, through row −1 of the view (`TerminalView::above_top`,
`crates/zyt-term-egui/README.md`), while tmux held more lines at the last
snapshot than the pane took (`Control::above_top`, `src/tmux/history.rs`).

| `tmux.history` | at row −1 |
| --- | --- |
| off — the default | the row says `tmux.load_history`, *load 24 lines of history*; a press loads them. The wheel stops on it and never loads: on a slow line a screen of history is seconds, and the echo of a key waits behind it |
| on | standing on the row loads the next screen at once; the view stays on its line, so the next load comes only when it reaches the row again |

A load snapshots the pane again, deeper by a screen: `display -p` first and
alone, since what to capture depends on the alternate screen, then

| on its reply | captured |
| --- | --- |
| off the alternate screen | `capture-pane -S -K`: history and screen, one range |
| on it | `-S -K -E -1`, the history; `-a -q`, the normal screen; then the alternate one |

`K` is the lines of history here and a screen more, no more than tmux holds.
The terminal is made anew and written whole; the one shown stays shown until
the new one is whole (`Pane::standing`), and the view is put back on its line of
text — lines come above it, so it stands the same number of lines from the
bottom (`Pane::keep_view`). While the load is on its way the row says
`tmux.loading_history`.

What a load costs: every press sends again all the history already here, and
the scrollback of the pane becomes the text of tmux — pictures, the marks of
OSC 133 and a gap in it go.

## The end

`ESC \` right after `%exit` ends the mode. The plain terminal is shown again
with what it held, at the size of the area, and the console is told that size:
it went to tmux while the mode stood. Title, directory and progress are those it
had before. The button of tmux pressed while leaving ends the mode at once.

A port lost does not end the mode: the far tmux cannot tell a pulled cable from
a quiet line. What was sent is forgotten, since a reply waited for that never
comes would move every later one onto the wrong command; when the port is back,
the size and the lists are asked again and the pane shown is snapshotted anew.

Control mode started inside a pane is cut out of what that pane shows, `detach`
is typed into the pane, and the desktop is told it is not supported.

A script is refused while the mode stands: the line carries the protocol.

The panes are not kept past the mode: no `%output` comes after the end, so
their terminals go stale at once, and the next attach rebuilds them from
snapshots; a session may never be attached again, and another client may be
attached meanwhile, so two copies of one pane would part. Kept through the end:
the plain terminal, the command history already written, the buffers of the
line, and the session attached to on its server; the session of this client
ends with it.

## What it says

Nothing of the mode is printed into a terminal; it goes to the desktop
(`docs/ui.md`, *What the window says, and where*): a start that failed with its
reason, a tmux too old, control mode inside a pane.

## The switch

`OscSettings::tmux_control`, the row *tmux control mode* (`DCS-1000p`) of
*Terminal Features*, is on in both columns: tmux on the other end of a serial
line is what it is for, and the mode reaches nothing past the terminal. What it
does that no other row does is make this side write to the line on its own,
which is why `Entering` waits for the first line before anything is written.
Off, `ESC P1000p` reaches the emulator as an unknown DCS, which swallows what
follows: a tmux started in control mode shows nothing. Turned off while the mode
stands, the mode is left with `detach`.

## Versions

What this side needs of tmux, and since when tmux has it:

| what | since | without it |
| --- | --- | --- |
| control mode, `capture-pane -p -e` | 1.8 | — |
| `%session-window-changed`, `%window-pane-changed` | 2.5 | the window shown is not followed |
| `send-keys -H` | **3.0** | no input: every byte typed goes through it |
| `mouse_sgr_flag`, `mouse_utf8_flag` | 3.0 | the mouse of a restored pane in the wrong encoding |
| `pause-after`, `%pause`, `%extended-output`, `refresh-client -f`, `-A` and `-B` | 3.2 | no flow control and no subscription of the bells: the lists carry the bells instead |

3.0 is the oldest served; the version is read first (`display -p
'#{version}'`), and an older one is told on the desktop and left with `detach`.
`refresh-client -C` is sent as `columnsxrows`, which 3.0a to 3.8 all take;
`refresh-client -f` is refused by 3.0a, whose flag is `-F`, which is why the
version gates it.

## What tmux was seen to do

Asked of every version in the container, by a program in a pane or by the lines
of the protocol themselves (`tmux-cc probe`, `experiments/tmux-cc`):

| question | seen |
| --- | --- |
| commands of one line | parted by ` ; `; `\;` is the form of a shell and reaches tmux as an argument |
| replies | one `%begin`/`%end` per command, and every command is answered, `send-keys` and `rename-window` too; the number is the server's, with gaps, and matches nothing this side sent |
| the first line after `ESC P1000p` | always `%begin … 0`; a start that fails answers that pair with `%error` and its reason, then `%exit` |
| a notification inside a reply, `%output` between the commands of a line | none, two panes flooding |
| `%output` of hidden windows, of other sessions | of a window not shown: it comes. Of another session: none |
| the order after `select-window` | to 3.4 the replies first, in 3.8 the notification first: a snapshot is sent on the notifications, not in the same line |
| `select-window` of the window current | no notification at all |
| `new-session -t '$S'` in control mode | moves the client to the new session of the group at once, `%session-changed` of it; the session starts on its first window. `set` with no `-t` after it in the line sets the new session. 3.0a to 3.8 agree |
| `destroy-unattached on` | the session ends once its last client detaches or switches away |
| `list-windows -a` in a group | a window once for each session of the group, with the window current in each |
| `resize-pane -Z -t %P` | toggles: zooms the pane and makes it active, or of a window zoomed unzooms it, whatever pane it names; `%window-pane-changed` and `%layout-change` with `Z` in the flags |
| `select-pane -Z -t %P` | in a window zoomed, the zoom moves to the pane; in one not zoomed, the pane is made active alone. 3.0a does not know `-Z`; 3.2a and later agree |
| `if -F` | one reply when no command runs, two when one does |
| a window closed in the session shown | `%unlinked-window-close` to 3.4, `%window-close` in 3.8: either closes it |
| `new-window` | `%session-window-changed` and `%window-add` alone when the size did not change: nothing names the pane but the lists |
| a tab in a format | written as `_`, so the field that may hold a space stands last |
| a `$` in a name | `\$` in 3.0a to 3.3a, `\\$` in 3.4, `$` in 3.8 |
| an argument beginning with `-` | read as a flag after the quotes are gone, so text and a new name go after `--` |
| `rename-window` | expands formats: `#` is written `##`; `send-keys -l` expands nothing |
| DA1 | `ESC[?1;2c`: no sixel, so a program that asks draws with characters |
| `CSI 18t` | rows first in 3.0a, 3.4 and 3.8; columns first in 3.2a and 3.3a |
| `capture-pane -e` with a hyperlink | SGR only to 3.3a; OSC 8 and a raw `ESC \` from 3.4, which is why the mode ends only on `ESC \` right after `%exit` |
| `pause-after` | a client not read at all: paused in 3.8, not in 3.3a; a client behind a slow line: never, *Flow control* |
| `refresh-client -A '%P:off'` | from 3.4 tmux stops reading a pane no client takes the output of, and its program blocks: not used |

Not yet checked on a Windows machine: the ConPTY the package carries
(`docs/sources.md`, *Consoles*).

## Memory

Once its buffers stand at their size the data path allocates nothing, with the
mode as without it (`docs/data-path.md`). A line of `%output` is decoded in
place in the buffer of the line, a line cut by the end of a chunk waits in the
client, and the output of a pane gathers in its inbox and is fed in the turn
of the screen;
a snapshot is written line by line, never gathered whole. What allocates is
rare and is control information: a name, a layout, a pane made. The inbox of a
pane and the line held by the client are brought to the read buffer as its own
buffers are.

## Settings

| what | where |
| --- | --- |
| the mode | `OscSettings::tmux_control`, *The switch* |
| how far a pane may reach past the area | `screen_overflow`, the file (`docs/config-only.md`) |
| history by scrolling or on demand | `tmux.history`, the block *tmux* |
| a pane that falls behind is paused | `tmux.pause`, the block *tmux*; how far behind, `tmux.pause_after`, the file |
| the shade of the panes out of focus | `tmux.shade`, the block *tmux* |
| the focus follows the pointer | `tmux.focus_follows_mouse`, the block *tmux*; off by default, *Panes side by side* |

The scrollback of every pane is the budget of a terminal, as for the plain one;
the mode has no keys of its own — the button of tmux and `RX` are the ways in.

## Tests

| layer | where | runs on |
| --- | --- | --- |
| the protocol | `crates/zyt-tmux`, unit tests and the recorded sessions | every target |
| the mode | `src/tmux/` against scripted lines; `src/session/tmux_tests.rs` with the fake device of a port | every target |
| the menu | `src/ui/tmux.rs`: its entries over a model, what each id chooses | every target |
| the mode end to end | `src/session/tmux_tests.rs`: a console running a real `tmux -CC` | unix |
| the protocol against tmux | `tests/tmux.rs`: typing, bytes of input, hidden windows, the edge of a snapshot, detach and attach | unix |
| the window | `src/ui/window_tests/`: the window under `egui_kittest` on the fake port — the menu chosen from by its labels, the button, `RX`, row −1 | every target |
| a slow line | `src/session/slow_tests.rs`: a real tmux behind `pty-throttle` at 9600 — a pane chosen and typed into, a bell of a hidden window, `pause-after` told, a flood without it losing nothing, history loaded | unix |

Every case against a real tmux runs on every version on the path: the container
builds `tmux` (3.3a), `tmux-3.0a`, `tmux-3.2a`, `tmux-3.4`, `tmux-3.8`.
