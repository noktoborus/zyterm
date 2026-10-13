# zyt-term

Terminal emulation driven by a byte stream.

## Scope

```
feed(&[u8])      ──► the alacritty state machine
render_into(&mut RenderableContent)  snapshot of the visible grid
take_output()    ──► bytes the terminal answers with (device queries)
take_events()    ──► title, bell, clipboard, directory, notification,
                     progress, shell mark, marked command, exit
set_sixel_placeholder(f)   what a picture this crate does not draw leaves
                           on the page in its place
set_cell_size(w, h)        pixels of one cell, which is how many cells a
                           picture comes to
set_default_colors(c)      what the page is painted in, which is what a
                           program asking (OSC 10, 11, 12) is told
cell_size()      ──► those pixels, read back for a caller that tells the
                           far side the size of its window
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

| what arrives | what it becomes |
| --- | --- |
| `BEL` of a program | an event; the one that terminates an OSC sequence is consumed by the parser |
| OSC 9 with a text | a notification, `NotificationKind` naming the sequence |
| OSC 777 | a notification with a heading and a text, named the same way |
| OSC 9 with a payload of `4;…` | not a notification but a `ProgressState`, the sequence ConEmu defined, so a program reporting percent raises no toast |

```
feed(chunk) ─┬─► the sniffer   a vte::Parser of its own, told about every OSC
             │                 SniffedReport: where each sequence was read
             └─► the parser    advanced to that offset, then the report is acted on
```

The sequences the backend drops are the ones the sniffer picks up, out of the
same bytes, with a parser of the same kind: `vte::Parser` as
`alacritty_terminal` re-exports it, driven by a `Perform` that implements
`osc_dispatch`, the hooks of a device control string and `csi_dispatch` for the
two control sequences the backend has no answer for. Terminators, parameters and a sequence split
across two chunks are therefore read the way the emulation reads them, and the
two cannot disagree about what a sequence was.

The command a shell marked is the line that was typed, which no sequence
carries: it is read out of the grid between `133;B` and `133;C`. The two walk a
chunk together because a mark says where it stands in the output, which is true
only while the bytes before it are drawn and the bytes after are not. The pass
stops at a sequence that said something and runs to the end of the chunk
otherwise, so output carrying none of them is one pass.

A payload is read as soon as it is whole, which is the `BEL` of one terminator
and the `ESC` of the other, so the offset of a sequence ended by `ESC \` leaves
that backslash ahead of it. It draws nothing, so a parser driven to the offset
stands where the report says it does.

### Sixel pictures

A sixel picture is a device control string, so the backend drops it. The sniffer
picks it up — [`Perform::hook`], `put` and `unhook` of the same pass — and the
crate draws none of it. What it does is hand the caller a `SixelReport`: the
bytes the payload came to, and the size its raster attribute declared, or
nothing where the payload declared none.

`set_sixel_placeholder` is a call and not a text, because the crate holds no
text of its own and because whatever the caller writes numbers with has to be
what writes these. It is handed the report and answers with the line to put
where the picture would have stood; a caller that says nothing sees a payload
drop as it always did.

That line is the picture and not a note about one. A picture takes the cells its
pixels come to — `set_cell_size` is how this crate knows how many that is, since
a cell is as large as a font it never sees — and the line is written through
those cells, row by row. The payload is kept (`Picture`), every cell of the
rectangle carries the number of it, so `Cell::image` answers which picture a
cell belongs to, `picture(id)` answers what that number names and
`Picture::cells` is the rectangle it stands in.

A picture wider than the room left on the line is fitted to that room when it is
placed: the pixels are untouched and the rectangle is smaller. After that the
cells are its own, and a page too narrow for it cuts them — the cells past the
new width are emptied before the grid is resized, so the reflow carries none of
them onto the line below. A page made wide again grows every row of it back to
the rectangle it was placed in and no further, into empty cells only: the pixels
never left, so what a cut took is marks. A picture is never less than one cell,
because something arrived. What a caller does with a payload — save it, hand it
to another program, copy it — needs no pixels, so it works whether anything ever
draws them.

The colours of the page are the caller's answer in the same way: a theme is the
interface, so `set_default_colors` is handed the three colours a reader sees and
`OSC 10`, `OSC 11` and `OSC 12` are answered with them. A caller that says
nothing leaves them unanswered, and an entry of the colour table (`OSC 4`) is
never answered.

How large the page is, in cells (`CSI 18 t`) or in pixels — the text area
(`CSI 14 t`) and one cell of it (`CSI 16 t`) — is answered from the grid and
`set_cell_size`, which is the only place the size of a cell is known. A program
that draws pictures asks one of the three before it draws, so a caller that
never says how large a cell is has them answered against `DEFAULT_CELL`.

A picture is drawn until a reader folds it (`fold_picture`): a folded picture is
kept and stands where it stood, `SnapshotPicture::folded` says so, and a
renderer draws no part of it, so what shows is the line that was written through
its cells.

| call | what it does |
| --- | --- |
| `picture(id)` | the payload and what it said, while the store still has it |
| `decode(report, payload)` | the pixels of a payload: `Pixels`, four bytes a pixel, transparent where the payload painted nothing |
| `pictures_held()` | what the pictures take, and what they may take |
| `set_picture_budget(bytes)` | changes that, dropping the oldest down to it at once |
| `clear_pictures()` | drops every picture, for a caller that clears the page and the scrollback |
| `fold_picture(id, folded)` | folds it into the line it left on the page, or unfolds it |
| `set_picture_access(a)` | `PictureAccess`: drawn, kept out of sight, or not read at all |
| `set_picture_limit(l)` | the size a larger picture is shrunk to, which is also what a program asking is told |
| `set_picture_weight(bytes)` | what one picture may weigh, payload and pixels together; a heavier one is refused |
| `DEFAULT_PICTURE_BUDGET`, `DEFAULT_PICTURE_WEIGHT`, `DEFAULT_PICTURE_LIMIT` | what the limits start at |
| `MAX_CANVAS` | the most a payload is read into before it is shrunk, unless the limit is larger |

The pixels of a kept picture are read as it arrives (`Picture::pixels`), not
when a caller asks: reading a screenful is milliseconds, and a caller asking is
something waiting. `decode` is public because it is what does that, and what it
reads is settled by two things the payload alone does not say — the size its
raster attribute declared, which the picture is cut to in both directions, and
whether the introducer asked for a transparent background, because a picture
that did not asks for its whole rectangle to be painted. `SixelReport` carries
both. The cut matters because a program lays its page out against the size it
asked for: the decoder works in whole bands of six, so a payload is a band
taller than it declared, and a picture a row or a column larger than that
stands over what was written beside it.

Both halves of a picture count against the budget. A picture past the limit is
shrunk to fit it, its proportions kept, and never enlarged. A payload of four
lines can declare a canvas of any size, so what is past `MAX_CANVAS` is cut away
before it is read. A picture heavier than the weight is kept not at all, and the
report says so (`SixelReport::refused`, `SixelRefusal::TooLarge`); so does one
where pictures are not read (`SixelRefusal::Ignored`). The placeholder is asked
for once that is known, so the line can say why. A payload the decoder cannot
read is kept not at all and is no refusal.

`PictureAccess::Kept` reads a payload and keeps it without putting the pixels in
the snapshot, so a renderer draws nothing and the caller can still save it or
copy it. `PictureAccess::Refused` reads nothing: the line of it names no
picture, the primary device attributes leave out the `4` that says sixel, and
`XTSMGRAPHICS` is left unanswered — a program that reads no answer reads it as a
terminal with no pictures. Where pictures are taken, that answer is the limit
above, so what a program is told is what it may send.

The number in a cell is a reference nothing holds: one whose picture the budget
has dropped reads as no picture, and the cells are then the text they always
were. That is what lets the oldest picture go without walking the scrollback to
find who pointed at it. The mark is kept in the zerowidth characters of the
cell, where a combining character goes, so the grid scrolls it, reflows it and
drops it off the end for free, and `selected_text` takes it out of the text the
way it takes out the marks of a run of NUL bytes.

The line is handed to the emulation a character at a time rather than fed to the
parser. The parser stands inside the terminator of the sequence at that point,
so bytes fed there would be read as the sequence its escape began; and a line
the caller wrote is text, so a control code in it must not be able to do
anything. It still wraps, scrolls and takes the colours of the moment, because
that is what the emulation does with a character.

Only the head of a payload is read, which is where the raster attribute stands.
A picture nobody draws therefore costs the bytes on the wire and nothing else.

[`Perform::hook`]: https://docs.rs/vte/latest/vte/trait.Perform.html#method.hook

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

| call | what it does |
| --- | --- |
| `scroll`, `scroll_to`, `display_offset`, `history_size` | where the page stands in the grid |
| `set_scrollback(lines)` | the cap, while the terminal runs; what is above it is dropped at once |
| `forget_scrollback()` | drops the lines above the screen and keeps the cap |
| `GRID_CELL_BYTES` | what one cell costs, because a row is kept whole at the full width and a caller budgeting memory counts in it rather than guessing |
| `selection_start(kind, column, row)` | selects the character at that place and keeps it as the anchor |
| `selection_update(column, row)` | moves the other end and leaves the anchor where it is |
| `selection_extend(column, row)` | what a press with `Shift` asks for, below |
| `set_selection_anchor(column, row)` | says where a selection *would* begin without starting one; the snapshot carries it as `selection_anchor` |
| `selected_text()` | reads the selection out |
| `selection_size()` | a `SelectionSize`: the characters of the longest line, the lines, and the characters with the line breaks left out |
| `select_by_key(step)` | starts or grows a **block** selection with the keyboard: the first step anchors at the cursor of the device, every later one moves the caret |
| `forget_key_selection()` | ends that gesture — the caret and the selection go, the anchor stays |
| `key_selecting()` | whether such a selection stands |
| `selection_edge()` | the end that moved last, where the page shows it: the pointer of a drag, the caret of the keys, and nothing while it is off the page |
| `selecting()` | whether a selection is being picked out at all, whichever way it was begun — what a caller reads to know the keys and the pointer are the selection's and not the device's |

Both ends of a selection take the whole of the character they stand on, so the
same two places give the same text whichever way it was made, and a selection
that moved nowhere is the one character it began on.

`selection_size` counts the text the selection would copy rather than the cells
it spans, so a count and a paste agree. Counting walks as much of the scrollback
as the selection covers, so the answer is kept until the selection names another
range or `feed` runs, and a caller may ask once a frame.

`selection_extend` jumps the anchor to the end of the selection further from the
character pressed on, and the selection then runs from the anchor to it. So a
press outside adds what lies between, a press inside cuts back to it, and the
selection never turns over. Which end is further is asked of the two ends, not
of a point between them, and asked again on every press. The anchor is left
there, so a drag that begins with the press grows from it, character by
character whatever the selection was made by. With nothing selected it starts
one at the anchor, and with no anchor either it does nothing.

An anchor is a place in the text, so it moves with the text and is nowhere while
its line is off the page. The caret of `select_by_key` is kept the same way, and
the snapshot carries both — `selection_anchor` and `selection_edge` — so a
renderer can mark where a selection began and where it is growing, and tell the
two apart.

`SelectionStep` is `Up`, `Down`, `Left` and `Right`, which walk by one cell or
one row, and `LineStart` and `LineEnd`, which are the first and the last cell of
the row the walking end stands on. Nothing wraps or leaves the grid.

A sideways step the caret cannot take — it is against the left or the right
edge — moves the anchor the other way instead, so a key held down goes on
widening the block. Up and down stop at the oldest line and the newest, and the
two ends of a row are places rather than directions and stop as well. The
viewport follows the caret.

### Search

| call | what it does |
| --- | --- |
| `search_set(query, SearchOptions)` | builds one pattern |
| `search_advance` | walks to the next match and wraps |
| `search_clear` | takes it down |
| `highlight_all` | marks every match of the visible rows (`Cell::matched`) |

A match becomes the selection, so it is highlighted and can be copied.

`SearchKind` is `Literal`, `Fuzzy`, `Word` and `Regex`, all in
`SearchKind::ALL`. `Word` is answered by the cells beside a match and not by the
pattern: the regex word boundary is rejected by the engine the backend builds,
and an ascii one would be blind to every alphabet but latin.

### Input and answers

| call | what it does |
| --- | --- |
| `take_output` | hands over the answers to what a program asked |
| `forget_output` | throws them away instead, for a caller giving up on everything on its way to the device |
| `encode_key`, `encode_paste`, `encode_mouse` | translate input into bytes |
| `ShiftEnter` | what `encode_key` sends for `Shift` with `Enter`: the carriage return of the key, `\x1b\r`, or `CSI 13;2u` — the key has no code of its own, so the spelling is the caller's |
| `answer_clipboard(Option<&str>)` | answers a request, or silently refuses it |

A program whose answers were thrown away is left waiting for one that never
comes, which is the cost of giving up on everything at once.

## Boundaries

Nothing about processes, ports or toolkits. Key and mouse types are defined
here, so the caller maps its own toolkit events onto them. No type of the
emulation backend appears in the public API.

## Errors

`Result<T, TermError>`: invalid grid sizes, positions outside the grid, a
search pattern that cannot be built (`InvalidPattern`), a payload that is no
picture (`NotAPicture`), and pixels that do not fill the picture they are shrunk
from (`PixelsMismatch`). Messages are English and
diagnostic only.
