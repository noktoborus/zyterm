# glyphs

Every glyph the fonts of egui carry, as a list.

## Scope

- Runs without a window: `egui::Context` is run for one frame with nothing in
  it, which is all the toolkit needs to build its fonts, so it works in a
  container or over ssh.
- It prints first which fonts the toolkit holds and which family each answers
  (`# loaded: 4`, a line per font), because a glyph belongs to a face and a face
  is reached through a family. A font no family asks for says so.
- Without an argument it lists the fonts egui ships with, a block per family.
  With a path it lists that file instead, bound to both families with nothing
  behind it, so what comes out is that font and not that font over the
  fallbacks.
- The list goes to standard output and to a file (`glyphs.txt`, or `--out`).
  The file is written before the glyphs are printed, so a reader that stops
  early still leaves it behind.
- A line is the code point, the glyph and the faces that carry it, tab
  separated: `U+2699\t⚙\temoji-icon-font`. A control character leaves its
  column empty.
- It asks the charmaps, not `Fonts::has_glyph`, which is the reason to have it:
  that one answers no for every code point whose first face in the chain is also
  the face the replacement glyph comes from, and it called the magnifier
  `U+1F50D` and the check mark `U+2714` missing while both are carried.

`src/ui/icons.rs` picks icons from this list rather than guessing —
`AGENTS.md`, `RULE icon.search`.

## Usage

```sh
cargo run -p glyphs                        # the fonts egui ships with
cargo run -p glyphs -- --out icons.txt     # the same, written elsewhere
cargo run -p glyphs -- /path/to/font.ttf   # one font file
```

## Boundaries

An experiment: nothing depends on it and it knows nothing of the application.
It installs nothing and asks the font service of the platform for nothing.

## Errors

`Result<T, GlyphsError>`: `Font`, `Write`, `NoGlyphs`. Each names the path it
was about; the tool prints it with its source and exits failing.
