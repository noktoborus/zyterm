# zyt-term-egui

egui widget for a `zyt-term` terminal.

## Scope

```rust
let (response, output) = TerminalView::new(
        &mut terminal, &mut content, &mut cache, &theme, &font)
    .focused(true)
    .links(true)
    .program_colors(true)
    .mouse_reports(true)
    .selection_anchor(false)
    .show(ui);
```

| flag | what it decides |
| --- | --- |
| `focused` | the block cursor against the hollow one, and whether the widget holds the toolkit focus |
| `links` | whether the link under the pointer is underlined and reported |
| `program_colors` | the theme alone, or the colours a program set over it |
| `mouse_reports` | whether a program that asked for the mouse is answered |
| `selection_ends` | whether the two ends of a selection are marked: a corner in the cell it began in and one in the cell it is growing into |
| `resize_delay` | how long the area stands still before the grid takes its size; until then the grid keeps its size and is drawn cut or short. The first size is taken at once; nothing is held back by default |

`TerminalOutput` carries:

| | |
| --- | --- |
| bytes | for the device |
| a grid size | after a resize |
| text | of a finished selection |
| a link | the one under the pointer, the one `ctrl` and the left button opened, the one the right button asked a menu for |
| a paste request | from the middle button |
| a flag | whether the program is reading the mouse |

`TerminalTheme::dark()` / `light()` is 22 colours; a cell the terminal marked as
a search match takes `search_match`, and the current match is the selection.
`program_colors(false)` draws that theme alone: the default pair, the cursor
colour and the palette entries a program painted over are all left unread, and
nothing else about the page changes.

A run of NUL bytes is drawn as one block: the colours of the cell exchanged, so
it stands out of whatever the text around it is painted in, and a frame in the
bright red of `TerminalTheme` round the whole of it, so the digits read as a
count and not as output. That red is the theme's own and never one a program
painted over — a program must not be able to paint the mark away.

The mark itself is drawn and not laid out: the `mark` module strokes a ring with
a line across it. Nothing is asked of a font, so the caller has nothing to
configure and a code point nothing carries cannot turn the mark into a box —
which is the same answer Firefox gives for a missing glyph (`gfxFontMissingGlyphs`,
the hexbox), reached with two shapes instead of a bitmap because one glyph is
needed rather than sixteen. The count beside it is ordinary text of the terminal
font. A run ends at the next mark, so two marks that met are two blocks.

`TerminalFont` carries the cell geometry. `map_key`, `map_modifiers` and
`map_button` convert egui input into the input types of `zyt-term`.

## Painting

`TerminalCache` keeps the page as one mesh. It is rebuilt when the page
differs, when the widget moved or was resized, and when the scale, font, theme
or atlas changed. A caller that replaces the fonts of the toolkit calls
`forget()`, because the mesh names glyphs by where they stand in the atlas.

- A touched terminal is not a changed page, so the cache renders into a buffer
  of its own and compares it with the page the caller holds. The whole page is
  compared — one mesh is built either way — and the buffers are then swapped,
  never copied.
- The cursor is drawn over the mesh every frame and left out of that
  comparison: it moves with nearly every byte and blinks. `focused()`, which
  decides block or hollow rectangle, therefore costs no rebuild.
- The link under the pointer is underlined over the mesh, because the pointer
  finds another link as often as it moves. It is underlined and reported
  whatever is held; `opens_link` is what a press on it answers to, and the hand
  of the pointer is shown while that is true and not whenever a link is under
  it.
- Characters are drawn by the toolkit, one shape per cell, so a letter of the
  terminal is the letter it is everywhere else in the window. Nothing of that
  layout is kept: a glyph held across the states in which the toolkit throws its
  own layouts away is a glyph drawn from the wrong place.
- Shapes are built first and tessellated afterwards, because laying out a
  character is what grows the atlas. The mesh buffer of the previous picture is
  reused, so its vertices are allocated once.
- A picture that grew the atlas asks for one more frame; one that grew nothing
  asks for none.
- A cell is painted in its own rectangle. The pixels left over beside the grid
  keep the widget background, so a row that paints itself stops where its last
  cell does.

## What the emulation is told

Two facts about the page are this side of the wall, and the widget hands both
over every frame: how large one cell is in pixels (`set_cell_size`, the font and
the scale of the screen) and what the page is painted in
(`set_default_colors`, the theme and whether a program may repaint it). The
emulation answers `CSI 14 t`, `CSI 16 t`, `OSC 10`, `OSC 11` and `OSC 12` with
them, so a program drawing pictures is told the same numbers the drawing uses.

## Pictures

A picture a program sent is a texture of its own, uploaded the first time it is
seen and kept for `KEPT_FRAMES` after the last one, since the snapshot stops
carrying a picture that has scrolled off the page while the terminal still holds
its pixels. They are kept in `TerminalCache` beside the mesh, because they are
kept the way it is: per view, across frames, and dropped with it.

One quad is drawn per run of cells. A cell of a picture carries which cell of
the picture it is, so the part to draw is read off the cell; neighbouring cells
of a row are merged, so a wide picture is one quad a row. The texture is
stretched over the cells, magnified with no filter and minified with one.

The texture holds the pixels that arrived and the quad holds the cells they
stand in, so a payload is read once however the window is resized.

A run is drawn only while it starts at the left edge of the picture
(`begins_a_row`): a run that does is the row, as much of it as the page holds.
The emulation cuts what a narrower page cannot hold and grows it back when the
page widens, so there is nothing of a picture on the line below; a run that
begins anywhere else is a terminal that kept its pictures out of sight and was
then asked to draw them. Which runs those are is worked out by `Drawn` before the text is laid
out, because the two answers are one: the page shows a picture or the line it
left, never both. Every cell of a picture that is drawn draws no character, the
cut-off ones among them; a page that drew both would be a picture with the words
behind it showing through wherever its pixels are transparent.

A cell of a picture nothing draws shows the line — one the store has dropped,
one a reader folded (`SnapshotPicture::folded`), one of a terminal that keeps
its pictures out of sight. The kept mesh is therefore told apart by the pictures
of the page as well as by its cells: folding one changes nothing else about the
page.

## Pointer

| input | what it does |
| --- | --- |
| drag | selects: 1 press cells, 2 words, 3 lines, decided by the press |
| drag held at the top or the bottom edge | the page creeps there, and the lines that arrive join the selection |
| `ctrl` + press | rectangle, and on a link it opens it |
| `ctrl+shift+alt` + press | rectangle, taken from a program reading the mouse |
| `shift` + press | moves the end of the selection; the start stays |
| held finger | selects the word, then adds what it moves over |
| middle button | reports a paste request |
| right button held | scrolls, text following the pointer |
| right button in place | asks for a menu |
| right button on a picture | reports `image_menu`, the number of the picture under it |
| wheel | scrolls |
| `alt` + wheel | sends `Up` or `Down`, and the view stays where it is |

The press count is counted here, on the press, because the toolkit decides a
double click on the release, after the drag has begun. Either `Alt` counts,
because `AltGr` is not reported as `Alt` where a layout has a third level.

A held finger is reported by the toolkit where a right click is, with the drag
taken away, so the selection reads `long_touched` and the pointer position.

A selection reaches what is on the page and nothing else, so a pointer held
against the top or the bottom of the view asks for the line beyond it. The band
that answers is measured in cells and its speed in rows a second — slowest where
the band begins, fastest at the edge of the view and past it — because what is
being asked for is rows. The page comes to the pointer, and the far end of the
selection then goes where the pointer stands, which is the row that just
arrived. A frame is asked for while the band holds the pointer: a pointer
standing still is no input, and nothing else would wake the window. In a view
too short for two bands and a middle the band is a third of it, so a selection
can still be dragged inside one.

A press that selects nothing leaves the anchor of a selection there.
`selection_ends(true)` draws a corner in that cell and another in the cell the
selection is growing into. The two point away from each other, so the pair
brackets what is taken from the outside, and which corner each takes follows the
block rather than the order the ends were made in: a selection dragged leftwards
or upwards is the same block as one dragged the other way, and a corner pointing
inwards would read as another block. Either way `shift` and a press select from
the anchor.

Whether the pointer belongs to a selection or to a program that asked for the
mouse is the caller's answer, through `mouse_reports`: `zyt-term::selecting`
says a selection is being picked out, and a caller that leaves the mouse to the
program while one is would be reporting a drag that picks out nothing.
The bytes of `TerminalOutput` are input — a press, the wheel, `alt` and the
wheel — and the widget does not end a selection for them: whether input ends one
is the caller's rule, and it holds for the keys as much as for the pointer.

`mouse_reports(false)` leaves a program that asked for the mouse unanswered, so
the pointer selects and scrolls as in a terminal nobody asked anything of. What
the program asked for is unchanged, so answering again hands it straight back.
A program that is answered also gets the motion between press and release.

The scrollbar takes the left button only — the right one over it is the page
being dragged — and lies over the text. It sets the offset from the end of the
output, so zero is "follow what arrives".

## Keyboard

While the widget is focused it holds the toolkit focus and locks Tab, the arrows
and Escape, so those keys stay available for the device. While it is not, it
stands outside the focus order altogether, or it would take the keyboard from
whatever holds it and never give it back. The mouse is answered either way.

## Boundaries

Key handling stays with the caller, because bindings are resolved before a key
may reach the terminal. The widget never touches a port, a process or a
configuration file, and performs no fallible operation, so it has no error type.
