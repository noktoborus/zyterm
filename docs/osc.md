# What a program can ask for

| sequence | answered by |
| --- | --- |
| title (OSC 0, 2) | the window title |
| clipboard (OSC 52) | the setting below |
| hyperlinks (OSC 8) | a link per cell |
| working directory (OSC 7) | the directory of the process |
| notifications (OSC 9, 777) | `notify-rust`; the two forms are chosen apart |
| shell marks (OSC 133) | the command history |
| progress (OSC 9;4) | the bar of the status bar, and the taskbar |
| colours (OSC 4, 10, 11, 12, 104, 110–112) | the palette of the terminal |

`Settings.osc` decides which are honoured, one choice per sequence and per kind
of session: a column for trusted output, a column for untrusted. `App::osc`
picks the column, so connecting elsewhere changes what is allowed.

Each choice is enforced where the sequence is acted on, never in the parser.
What is refused is dropped, not buffered, and a refusal is silent.

| sequence | enforced in |
| --- | --- |
| clipboard | `zyt_term::Terminal` |
| title, notifications | `App::handle_terminal_events` |
| links | `TerminalView::links` |
| colours | `TerminalView::program_colors` |
| shell marks | `Session::marks_enabled` |

A colour a program set travels in the snapshot and not in the theme, so refusing
it is one `if` at the point of drawing and needs nothing repainted or reparsed
(`crates/zyt-term/README.md`).

## Trust

Trust is `Console.trusted`, a judgement about a console and not a fact about the
machine: a console carrying somebody else's output — an `ssh`, a log — is not
trusted wherever it runs, and a device never is. The sign in the status bar is
also the switch (`App::toggle_session_trusted`); on a line it cannot be pressed.
Untrusted starts at copy-only clipboard, with notifications, title, links and
colours refused, keeping only its shell marks. `App::open_link` refuses a
`file://` path while the session is untrusted, and the terminal says why.

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
