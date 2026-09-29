# zyt-term

Terminal emulation driven by a byte stream.

## Scope

```
feed(&[u8])      ──► the alacritty state machine
render_into(&mut RenderableContent)  snapshot of the visible grid
take_output()    ──► bytes the terminal answers with (device queries)
take_events()    ──► title, bell, clipboard, directory, notification,
                     progress, shell mark, marked command, exit
is_dirty()       ──► whether anything changed since the last snapshot
```

The snapshot is in types owned by this crate — `Cell`, `Color`, `CellStyle`,
`CursorInfo` — and hyperlinks are stored once per snapshot, addressed per cell
by `LinkId`.

The colours a program set travel with it rather than in the cells:
`background`, `foreground` and `cursor_color` are the ones it named, and
`palette` holds the entries of the 256 colour table it painted over — either
`PALETTE_COLORS` entries or none at all, because a table nobody painted is
empty. A renderer that prefers them over its own theme obeys `OSC 4`, `10`, `11`
and `12`; one that ignores them draws its theme and nothing is lost.

### Events

The `BEL` of a program is an event; the one that terminates an OSC sequence is
consumed by the parser. A notification names the sequence that asked for it
(`NotificationKind`), because OSC 9 carries a text and OSC 777 a heading and a
text. OSC 9 with a payload of `4;…` is not a notification but a progress report
(`ProgressState`), the sequence ConEmu defined, so a program reporting percent
raises no toast.

The sequences the parser drops (OSC 7, 9, 133, 777) are picked up by a sniffer
that scans the same bytes for the escape byte with `memchr`. The command a shell
marked is the line that was typed, which no sequence carries: it is read out of
the grid between `133;B` and `133;C`. That is why `feed` walks a chunk with the
parser and the sniffer *together* — a mark says where it stands in the output,
which is true only while the bytes before it are drawn and the bytes after are
not. `SniffedReport` carries the offset where each sequence ended; a chunk with
none of them is still one pass.

### Runs of NUL bytes

A NUL byte is a character the standards tell a terminal to ignore, so it reaches
no cell. This crate writes one run of them into the grid instead. A device that
went quiet in the middle of a word, a line read at the wrong speed and a flash
image sent to a console all say nothing otherwise, and that nothing is what the
reader is looking for.

| run | cells |
| --- | --- |
| one byte | the mark alone |
| more than one | the mark, `MULTIPLICATION SIGN`, the decimal count |

One byte carries no count because `×1` says what the mark has said already,
and the two cells it would take are two cells of a line somebody is reading.

The cells carry noncharacters (`U+FDD0`..), which Unicode promises never to
assign, so a cell holding one came from here: the private use area could not
promise it, being where the fonts of a prompt keep their arrows.

| call | answers |
| --- | --- |
| `null_part(ch)` | `NullPart::Mark`, `Times`, `Digit(n)`, or nothing |
| `NullPart::text()` | the character to draw, and **nothing for the mark** |
| `NULL_SYMBOL` | the code point the mark stands for, `U+2400` |
| `TIMES_SIGN` | the character between the mark and the count, `U+00D7` |
| `null_text(ch)` | what a cell reads as outside the grid |

The mark has no character on purpose. A code point is a glyph only where a font
carries one and no font can be promised, so `text()` answers `None` and the
renderer draws the mark itself; everything else is a character, so the count is
drawn in the same font as the line it stands in. `selected_text` and a command a
shell marked carry `NULL_SYMBOL` and plain digits, so a block read on the screen
is a block wherever it was pasted.

A run is counted inside one chunk of `feed`. A run split over two reads is two
blocks, because the first is on the screen before the second read happens.

### Scrollback and selection

- `scroll`, `scroll_to`, `display_offset`, `history_size`.
- `set_scrollback(lines)` may be called while the terminal runs and drops what
  is above the new cap at once; `forget_scrollback()` drops the lines above the
  screen and keeps the cap. `GRID_CELL_BYTES` is what one cell costs, because a
  row is kept whole at the full width and a caller budgeting memory counts in it
  rather than guessing.
- A selection begins *before* the character it was started on, so the first one
  it takes is the one that was pointed at. `selected_text()` reads it out.
- `selection_extend(column, row, right_half)` moves the far end, or starts one
  at the anchor when nothing is selected — what a press with `Shift` asks for.
  The place it began at stays, so one press grows it and the next shrinks it.
- `set_selection_anchor(column, row)` says where a selection *would* begin
  without starting one; the snapshot carries it as `selection_anchor`. It is a
  place in the text, so it moves with the text and is nowhere while its line is
  off the page.

### Search

`search_set(query, SearchOptions)` builds one pattern, `search_advance` walks to
the next match and wraps, `search_clear` takes it down. A match becomes the
selection, so it is highlighted and can be copied; `highlight_all` marks every
match of the visible rows (`Cell::matched`).

`SearchKind` — `Literal`, `Fuzzy`, `Word`, `Regex`, all in `SearchKind::ALL`.
`Word` is answered by the cells beside a match and not by the pattern: the regex
word boundary is rejected by the engine the backend builds, and an ascii one
would be blind to every alphabet but latin.

`take_output` hands over the answers to what a program asked; `forget_output`
throws them away instead, for a caller giving up on everything on its way to the
device — those bytes being on their way too. The program is then left waiting for
an answer that never comes, which is the cost of asking for everything.

`encode_key`, `encode_paste` and `encode_mouse` translate input into bytes;
`answer_clipboard(Option<&str>)` answers or silently refuses a request.

## Boundaries

Nothing about processes, ports or toolkits. Key and mouse types are defined
here, so the caller maps its own toolkit events onto them. No type of the
emulation backend appears in the public API.

## Errors

`Result<T, TermError>`: invalid grid sizes, positions outside the grid, and a
search pattern that cannot be built (`InvalidPattern`). Messages are English and
diagnostic only.
