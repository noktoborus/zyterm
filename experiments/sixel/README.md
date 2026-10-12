# sixel

What a sixel payload is, what it would cost to draw, and what it looks like.

## Scope

```sh
cargo run -p sixel -- make 64 36 > pattern.six   # a payload of known content
cargo run -p sixel -- show pattern.six           # what it is, and what it costs
cargo run -p sixel -- blocks pattern.six         # the same, and the picture
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
