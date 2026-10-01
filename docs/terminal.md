# Terminal

The emulation is `crates/zyt-term/README.md` and the widget drawing it is
`crates/zyt-term-egui/README.md`: the snapshot types, the runs of NUL bytes, the
parser walking beside the OSC sniffer, the selection calls and what the cache
compares are all written down there. This chapter is what the application
decides.

`zyt-term` drives `alacritty_terminal::Term` through `vte::ansi::Processor`. No
process is involved, so the same emulator serves a serial line, a console or a
test buffer.

The sniffer of the sequences that backend drops is a second `vte::Parser`,
reached through the re-export of `alacritty_terminal` and never declared as a
dependency of its own: two parsers of two versions would read one stream two
ways.

## Scrollback

Scrollback is a memory budget, not a line count, because a row is kept at the
full width of the window whatever stands in it (`GRID_CELL_BYTES` per cell).

| | |
| --- | --- |
| `Settings.scrollback_memory` | the budget, in mebibytes |
| `config::scrollback_lines` | turns it into lines at the current width |
| `App::apply_scrollback` | hands that to the grid every frame, because every frame may be a new width |
| `App::follow_connection` | `Terminal::forget_scrollback` when a session ends |

## Rendering

```
Terminal ──render_into──► RenderableContent ──build_picture──► Mesh ──► egui
             (visible page only)              (cached between frames)
```

Only the visible page is ever rendered. Scrollback lives in the grid; scrolling
changes `display_offset` and the next snapshot covers the new page.

`TerminalCache` keeps the grid as one mesh, and the `program_colors` and `links`
flags of the widget are part of what it compares (`Painted`), so turning either
off is a rebuild and nothing else.

The glyph atlas is what the cache rests on: a mesh names each glyph as a share
of the atlas image, so an atlas that grew moves all of them.

| event | when the picture is rebuilt |
| --- | --- |
| page changed | same pass |
| palette changed | pass of its own (`App.theme_settling`) |
| fonts replaced | pass of its own (`App.fonts_settling`) |
| atlas grew | next pass (`App::watch_the_atlas`) |

Fonts take effect only in the pass after they are handed over, and a palette
change happens mid-pass, so both defer the rebuild rather than cut the picture
against an atlas that is about to move.

`App::raw_input_hook` caps the atlas side at `render::ATLAS_SIDE`. epaint
doubles the atlas height as glyphs arrive, so every step costs the full width,
in memory and on the chip.

`wgpu` reaches the chip through Vulkan, DirectX or OpenGL. `src/render.rs` picks
the adapter by hand — a power preference never rules out a software adapter —
and cuts the requested limits down to what the adapter offers, because the
`wgpu` defaults exceed what a small chip such as the Raspberry Pi V3D grants.
Texture sizes are asked for in full. A machine with no chip still gets a window
and the log says so.

## Fonts

Nothing is installed and nothing is copied: a font is mapped, not read.
`FontData::from_static` over a mapping is held by reference (`Blob` is
`Arc<dyn AsRef<[u8]>>`), so pages are read as they are touched. Measured on a
0.7 MiB font: 2.4 MiB held, 1.4 MiB mapped. `fonts::mapped` maps each file once
for the life of the process.

```
terminal  FontFamily::Name("terminal")  chain of families, first is the grid
interface Proportional + Monospace      one family
          ↓ behind both
          the fonts egui ships with, then fonts::add_fallbacks (locale scripts)
```

The terminal takes a chain because one family rarely carries everything a device
prints, and every link must be monospaced. The interface is one family and may
be any of them: it is prose. The terminal family is always bound, with the
toolkit's monospaced fonts behind or instead of the choice, because a family
nothing is bound to panics the first time a glyph is asked for.

`fontdb` reads the families only where somebody asked for the list:
`App::font_families` reads them when one of the two menus is opened, never for
drawing, because walking the font directories is the longest wait a window has.
So a row of the chain is just a name, and the lists that ask answer the rest —
the add list offers monospaced families only, the interface list marks a chosen
family as gone. Monospacing is read from face metadata, not by loading files. A
name that names nothing is logged and skipped, and the settings still show it.

Fonts are replaced between passes, never inside one (`App::apply_fonts` runs
before the frame). The first window is settled in `App::new`.

`Settings.font_size` and `interface_font_size` are points. The interface size is
the body text; `App::apply_interface_size` derives the other styles from the
toolkit's own ratios, always from the shipped sizes and never from the current
ones, which would grow a little every frame.

## Themes

`TerminalTheme` is 22 colours. `Built-in dark` and `Built-in light` are compiled
in; the rest are files in `<config>/themes` in the Alacritty colour format.
`ThemeCatalog` reads the directory at startup and on demand, keeps the built-ins
first and names files it could not read instead of failing. What a file leaves
out keeps the value of the built-in of its mode, and the background decides that
mode.

`Settings.themes` names one palette per colour mode;
`App::apply_terminal_theme` resolves it when the mode or the setting changes,
falling back to the built-in when the name resolves to nothing.

The palette is the terminal's alone — the interface takes `egui::Visuals`. Every
panel takes the terminal background, so a program that paints itself dark (OSC
10, 11) leaves no band of the theme beside it, and the scrollbar lies over the
text.

`dark-light` reports the desktop setting and is subscribed to — one D-Bus signal
of the portal on Linux — falling back to a once-a-minute read where the platform
cannot notify. Nothing queries D-Bus per frame.
