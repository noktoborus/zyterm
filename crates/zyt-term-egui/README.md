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
    .null_glyph('\u{2400}')
    .show(ui);
```

`TerminalOutput` carries bytes for the device, a new grid size after a resize,
the text of a finished selection, the link under the pointer, a paste request
and whether the program is reading the mouse.

`TerminalTheme::dark()` / `light()` is 22 colours; a cell the terminal marked as
a search match takes `search_match`, and the current match is the selection.
`program_colors(false)` draws that theme alone: the default pair, the cursor
colour and the palette entries a program painted over are all left unread, and
nothing else about the page changes.
A run of NUL bytes is drawn as one block: the colours of the cell exchanged, so
it stands out of whatever the text around it is painted in, and a frame in the
bright red of `TerminalTheme` round the whole of it, so the digits read as a
count and not as output. That red is the theme's own and never one a program
painted over — a program must not be able to paint the mark away. `null_glyph`
is the glyph the mark is drawn as; the caller asks its fonts which of the code
points it has, because a code point no font carries is drawn as a box.

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
  finds another link as often as it moves.
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

## Pointer

| input | what it does |
| --- | --- |
| drag | selects: 1 press cells, 2 words, 3 lines, decided by the press |
| `ctrl` + press | rectangle |
| `ctrl+shift+alt` + press | rectangle, taken from a program reading the mouse |
| `shift` + press | moves the end of the selection; the start stays |
| held finger | selects the word, then adds what it moves over |
| middle button | reports a paste request |
| right button held | scrolls, text following the pointer |
| right button in place | asks for a menu |
| wheel | scrolls |

The press count is counted here, on the press, because the toolkit decides a
double click on the release, after the drag has begun. Either `Alt` counts,
because `AltGr` is not reported as `Alt` where a layout has a third level.

A held finger is reported by the toolkit where a right click is, with the drag
taken away, so the selection reads `long_touched` and the pointer position.

A press that selects nothing leaves the anchor of a selection there.
`selection_anchor(true)` draws a bar at it; either way `shift` and a press
select from it.

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
