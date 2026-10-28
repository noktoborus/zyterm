# What a program can ask for

| sequence | answered by |
| --- | --- |
| title (OSC 0, 2) | the window title |
| clipboard (OSC 52) | the setting below |
| hyperlinks (OSC 8) | a link per cell, opened by `ctrl` and the left button |
| working directory (OSC 7) | the directory of the process |
| notifications (OSC 9, 777) | `notify-rust`; the two forms are chosen apart |
| shell marks (OSC 133) | the command history |
| progress (OSC 9;4) | the bar of the status bar, and the taskbar |
| colours (OSC 4, 10, 11, 12, 104, 110–112) | the palette of the terminal |
| pictures (DCS q, sixel) | the setting below, one of three |
| the display mode of pictures (CSI ? 80 h, l, $ p) | where a picture is placed (docs/terminal.md) |
| dark or light (CSI ? 2031 h, CSI ? 996 n) | the theme of the window |

`Settings.osc` decides which are honoured, one choice per sequence and per kind
of session: a column for trusted output, a column for untrusted. `App::osc`
picks the column, so connecting elsewhere changes what is allowed.
`config::OscSequence` is the list — what each sequence is called, which numbers
it answers and in which order they are read — and both the settings page and the
plate of the sign of trust are drawn from it, so neither can name a sequence the
other does not.

The working directory is the one switch whose column default is *off* where a
file says nothing (`#[serde(default)]` rather than `honoured`): honouring it
moves the working directory of this process on the word of the far side, and the
file dialog, a dropped file and a new window all read it afterwards.

Each choice is enforced where the sequence is acted on, never in the parser.
What is refused is dropped, not buffered, and a refusal is silent.

| sequence | enforced in |
| --- | --- |
| clipboard | `zyt_term::Terminal` |
| pictures | `zyt_term::Terminal`, before the decoder |
| title, notifications | `App::handle_terminal_events` |
| the working directory | `App::handle_terminal_events`, before `enter_directory` |
| links | `TerminalView::links` |
| colours | `TerminalView::program_colors` |
| shell marks | `Session::marks_enabled` |

A link is underlined and its address read in the corner wherever the pointer
crosses one, and it takes `ctrl` and the left button together to open it
(`view::opens_link`). The button alone is a press on the text the link is drawn
over — a word of it is double-clicked, a run of it is dragged over — and what
the far side printed must not reach the desktop on one click of a press that was
meant for the terminal. The right button asks for the menu of the link whatever
is held: a menu is a question and not an act, and `App::open_link` is still what
every entry of it goes through.

## Links

Two switches of a column and not one, because they answer two questions:

| switch | what it decides |
| --- | --- |
| *Links* (OSC 8) | whether a link a program marked is a link at all: underlined, named in the corner, with its menu and `ctrl` and a click |
| *Local links* (`OscSettings::local_links`) | what is done with a link to a file of this machine, marked by a program or read out of a selection |

Where a link leads is `zyt_files::place_of`:

| place | what it is | *Local links* on | off |
| --- | --- | --- | --- |
| `Here(path)` | `file://` with no host, `localhost` or the name of this machine; on Windows `C:\…` and `C:/…`, a drive of the network as well | the menu of the file or the directory when it is there; copied when it is not | copied |
| `Share` | `\\server\…` on Windows | opened, opened with, copied — the server is never asked anything before | copied |
| `Away` | `scheme://…`, `mailto:`, `file://` of another host | opened, opened with, copied | the same |

Off, a link to a file is taken as one of the other end: the path is the word of
that end, and opening it here would open a file of this machine on its say-so.
So `App::refuses_path` refuses it, from a menu and from `ctrl` and a click
alike, and the terminal says why. *Local links* is on for trusted output and off
for the rest; a column of a file that says nothing about it starts at off, for
the same reason.

A selection is a link when the whole of it is one, the spaces at its edges
taken off (`zyt_term::is_link`): `scheme://…`, `mailto:…`, and on Windows the
two paths above. A path of Unix is not one. A right click while a selection
stands asks the screen for its text (`Purpose::SelectionMenu`), and the menu
opens when the answer comes, the way the menu of a picture does
(`ui::terminal::open_selection_menu`). A link marked under the pointer or a
picture comes first. A link whose menu could only copy it opens the menu of the
terminal, which copies the selection already. A double press on a link takes
the link whole (`crates/zyt-term/README.md`).

A colour a program set travels in the snapshot and not in the theme, so refusing
it is one `if` at the point of drawing and needs nothing repainted or reparsed
(`crates/zyt-term/README.md`).

## Pictures

`Settings.osc.*.sixel` is three values and not a switch
(`config::SixelSetting`), because what a picture costs and what it does are two
questions:

| value | what the far side gets |
| --- | --- |
| show the picture | it is read, kept and drawn where it stands |
| do not show the picture | it is read and kept, and the line of it stands in its place; the menu of it is still the menu of a picture |
| ignore the picture | it is not read at all, and the line is text that names nothing and says it was ignored |

The middle one is what an untrusted session starts at. A picture it sends can
still be saved, handed to a viewer or copied — that is what reading it is for —
and what the window looks like stays this side of the line. The page and the
plate of the sign of trust both name the value that stands, the way they do for
the clipboard, since a row reading "pictures" would leave the one question
unanswered.

Refusing is refused all the way: nothing is decoded, nothing is kept, the
primary device attributes leave the `4` out, and a program asking what pictures
this terminal takes (`XTSMGRAPHICS`) is not answered at all — a program that
reads no answer reads it as a terminal that has none. What is answered where
pictures are taken is `zyt_term::PictureLimit`, which is also the size a larger
picture is shrunk to, since a program that believes the answer sends what we
keep.

## The colours of the page

A program asking what the page is painted in — the foreground (`OSC 10`), the
background (`OSC 11`), the cursor (`OSC 12`) — is told what the window actually
paints with. The colour is the interface's: a theme, which follows the desktop,
and the setting that says whether a program may repaint it at all. So the
renderer hands the three colours over (`Terminal::set_default_colors`, from
`TerminalView::show`, the way it hands over the size of a cell) and the
emulation answers with what it was given — the colour a reader sees, with no
second opinion about it.

A program that is told nothing here draws against a colour it guessed. `lss`
asks for the background and cuts it out of every thumbnail, so a wrong answer is
a halo round each of them; no answer at all leaves it at its own default.

An entry of the 256 colour table (`OSC 4`) is not answered. Those colours are
the theme's as well, and nothing has asked for them yet.

A program that sets private mode 2031 is told when the theme turns dark or
light: `CSI ? 997 ; 1 n` or `CSI ? 997 ; 2 n`. Claude Code with the theme
`auto` asks for the background (`OSC 11`) on this report and picks its colours
by it. The flow:

```
desktop or settings ──► App::apply_terminal_theme ──► TerminalView::show
    set_default_colors (OSC 10, 11, 12 answer the new colours)
    set_color_scheme   (CSI ? 997 ; n goes to the line, if mode 2031 is set)
```

The order matters: the program asks for the colours as soon as it reads the
report, so the colours are handed over first. Dark or light is read from the
background of the theme and not from the background a program painted
(`ColorScheme::of`): a program that repaints the page is not told of its own
colour. It is not behind a setting, because the colours it leads to are
answered to every program anyway. Only the terminal shown is told, since only
it is drawn; a terminal shown later is told then.

## Trust

The sign of trust carries no hint. What one would say — the name of the column —
is the heading of a plate the pointer raises instead, and under it stand the
sequences this session is allowed, by the names the settings page gives them;
what is refused is left out, because the question the sign raises is what a guest
may do. The clipboard is four settings rather than a switch, so its row names the
one that stands — *copy only*, *copy and limited paste*, *copy and paste* — and
not the clipboard itself, which the row would otherwise leave unsaid. The right button leaves the plate standing, since the left one is the
switch (`statusbar::trust_plate`, `UiState::trust_plate_pinned`), and it is held
down until the pointer leaves for the reason the plate of the times is.

Trust is `Console.trusted`, a judgement about a console and not a fact about the
machine: a console carrying somebody else's output — an `ssh`, a log — is not
trusted wherever it runs, and a device never is. The sign in the status bar is
also the switch (`App::toggle_session_trusted`); on a line it cannot be pressed.
Untrusted starts at copy-only clipboard, with notifications, title, links, local
links and colours refused, keeping its shell marks, the progress of a program,
and tmux control mode, which is what a device on a line runs it for
(`docs/tmux.md`). What local links decide is under *Links* above.

## The clipboard

| setting | storing | reading |
| --- | --- | --- |
| disabled | no | no |
| copy only | desktop clipboard | no |
| copy and limited paste | desktop clipboard + `App.stored_clipboard` | from `stored_clipboard` |
| copy and paste | desktop clipboard | the desktop clipboard |

Limited paste answers without asking the toolkit, so a program gets back what a
program of this window stored and what the user copied elsewhere stays where it
is. To the terminal it looks like `ClipboardAccess::CopyPaste`, because the
difference is who answers, not whether reading is honoured.

The clipboard belongs to the toolkit, so paste and an honoured request both use
`ViewportCommand::RequestPaste` and answer when the paste event arrives.

## The bell

`BEL` asks for the person, so it asks the desktop:
`ViewportCommand::RequestUserAttention` when the window is not focused, and
nothing at all when it is. The flag is taken down here on the first focused
frame — the toolkit says it resets on focus, but that belongs to the window
manager — and `App.attention_asked` keeps it from being sent every frame.

## Icons

Every icon is a code point in `src/ui/icons.rs`, and a test there asks the
toolkit whether its fonts carry each one: only two of the fonts egui ships with
answer the proportional family, so a plausible code point is as likely to be
drawn as a box. The test asks the charmaps of the family, not
`Fonts::has_glyph`,
which answers no for a code point whose first face is the one the replacement
glyph comes from. `experiments/glyphs` lists what the fonts do carry.
