# sixel

What a sixel payload is, what it would cost to draw, and what it looks like.

## Scope

```sh
cargo run -p sixel                               # every layout, space steps on
cargo run -p sixel -- demo beside                # one of them, to standard output
cargo run -p sixel -- make 64 36 > pattern.six   # a payload of known content
cargo run -p sixel -- show pattern.six           # what it is, and what it costs
cargo run -p sixel -- blocks pattern.six         # the same, and the picture
script -q -c 'lss -ly' /tmp/lss.raw              # what a program wrote
cargo run -p sixel -- replay /tmp/lss.raw --cell 9x17 --grid 110x36
```

- `show` reads a payload — a file, or standard input — and says what one pass
  over it found:

  ```
  payload           918 bytes
  raster attribute  1:1 aspect, 64 x 36 declared
  pixels            64 x 36
  cells at 8x17     8 columns x 3 rows
  colour registers  7 (7 of them painted by the payload)
  pixels drawn      2304 of 2304 (0 left transparent)
  runs              110 horizontal runs of one colour, 20.9 pixels each
  as a texture      9216 bytes of RGBA
  decoded in        239.3µs
  ```

- The payload is read the way the terminal will have to read it: the tokenizer
  byte by byte, the deserializer told about every event. So a payload this tool
  reads is a payload the terminal reads, and the other way round.
- `runs` is the number of coloured rectangles the picture comes to once equal
  neighbours of a row are merged. It is the number the choice between a texture
  and plain geometry turns on: device graphics come to a few hundred, where a
  photograph comes to a rectangle per pixel.
- `--cell WxH` counts the columns and rows against that cell instead of the
  default, because how many cells a picture takes is a fact about the font and
  not about the payload.
- The viewer is what runs when nothing is asked for: the layouts are what this
  tool has to show, and reading a payload is what it is told to do.

- `demo` shows every layout in the window it is run in, one press of the space
  bar apart: the same window, the same font, one layout after another, so what
  differs between them is the layout and nothing else. `q`, the escape and `^C`
  leave, and the terminal is put back the way it was found. What it writes is
  the bytes a program sends to put pictures on a page that way — nothing is
  drawn by this tool — and what it is for is holding a terminal to the layouts
  programs actually write.

  `demo <name>` writes one of them to standard output instead, which is how a
  layout is captured or piped, and `demo list` names them.

  | layout | what it shows |
  | --- | --- |
  | `plain` | a picture of its own, a line of text above it and one below |
  | `beside` | room made with line feeds, the cursor back up, the picture between a save and a restore, the text to the right of it with `CSI n C` — how a file listing writes a thumbnail, `lss -ly` among them |
  | `row` | thumbnails side by side on one band of rows, each caption under its own |
  | `inline` | a picture one row tall inside a sentence, which is what an icon of a prompt is |
  | `sizes` | the same pattern at five widths, each line saying what it asked for |
  | `over` | text written where a picture stands with no cursor move of its own, which takes those cells back from it |
  | `scroll` | ten pictures one after another, so the page scrolls under them |

  The layouts say nothing about where a terminal leaves the cursor after a
  picture, except `plain` and `over` which are about exactly that. A window
  narrowed over any of them says more about the terminal than a payload on its
  own does: what the page holds of a picture is drawn and the rest is cut off,
  while what a program laid out beside it wraps.

  `--cell WxH` lays them out against that cell instead of the default; a
  terminal answers `CSI 16 t` with its own.

- `replay` feeds a captured stream to `zyt_term` — the same emulation the window
  draws from — and prints the page as one character per cell: `#` a cell of a
  picture, `-` a cell of one that shows no part of it, `.` a space, everything
  else the character itself. Above the page it says what every picture declared,
  what it was read as, and how many cells it stands in.

  It is how a layout that comes out wrong is read, because the pictures are in
  the way of the question: which cell holds what. The stream is captured with
  `script`, which records what a program wrote and answers what it asked, and
  the cell and the grid are the ones the window had — `lss --diag` says what a
  program was told, and the settings page says what the window is.

- `blocks` draws the picture with `UPPER HALF BLOCK` and the colours of the
  terminal, two rows of pixels per row of text. It is how this tool is read; it
  is not what the terminal will do with a payload.
- `make` writes a pattern of known content: colour bars, a frame one pixel wide
  and a diagonal. The frame says where the picture ends, which is what placing
  it into cells is read against — a picture one pixel short of a row of cells
  leaves part of a row empty. The diagonal says which way up it was drawn and
  whether anything was scaled unevenly. It is written by hand and not by the
  serializer of the crate, because a payload both written and read by the same
  code says nothing about either.

## What it answered

- The crate wants the whole sequence, `\x1bPq` and the terminator included, so a
  payload taken off a `DCS` hook has to be handed that intro back.
- `SixelImage::pixel_size` answers `(height, width)`, in that order.
- The decoded picture is as tall as whole bands of six pixels, whatever the
  raster attribute declared: a payload that says 7 rows decodes to 12. A
  terminal that places it has to clip to the declared height, or the picture
  takes a row of cells nobody asked for.
- The raster attribute is not on `SixelImage` — the crate keeps it to itself —
  so a caller that wants the declared size reads it off the tokenizer events.
  That is also how a refused payload is described without building an image.
- Colour registers are said in hundredths, and may be said in hue, lightness
  and saturation. Both are turned into channels of a byte here.

## Boundaries

It knows nothing of this application: no window, no terminal emulation, no
settings. It reads bytes and writes text to standard output.

## Errors

A payload that is not sixel, a file that cannot be read and an argument that is
not a size are a line on standard error and a status of one.
