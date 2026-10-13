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

## Resizing

The grid takes the size of its area once the area has stood still for
`Settings.resize_delay` (`zyt_term_egui::TerminalView::resize_delay`), not on
every frame it changes. A window dragged across forty columns is forty frames,
and a grid that followed each was forty reflows and forty new sizes told to the
program:

- a shell redraws its prompt on each, moving up as many rows as it believes the
  prompt took at the size before, and the reflow has made that count wrong;
- a device on a line does not know the size at all and redraws its menu at
  fixed positions over a page the reflow has moved, so what it drew before
  stays where it was.

Held back, the grid keeps the size the program was last told and is drawn at
it, cut by the area or short of it. Output goes on arriving and lands at that
size, which is the size the program is drawing for. When the area stands still
the grid is reflowed once and the program is told once. foot does the same with
`resize-delay-ms`; it pauses reading the console meanwhile, which a serial line
cannot be asked to do and which is not needed while the grid keeps its size.

The first size of the area is taken at once, so a window opened at another size
than the configuration says does not show its first lines in a corner. The key
is in the configuration file and not on the page: `docs/config-only.md`.

## Sixel pictures

A sixel picture is a device control string, which the backend drops: it reaches
no cell and is written to the log and nowhere else. The sniffer picks it up out
of the same bytes, so what arrived is known — the size the raster attribute
declared, and what the payload came to on the line.

What the terminal does with a payload is keep it, take the cells its pixels
come to, and write a line of text through those cells:

```
a <sixel 64
x36, 918 B>
 b
```

How many cells that is follows the size of a cell, which only the renderer
knows: it says so with `Terminal::set_cell_size`, in pixels of the screen and
not points, the way it says the columns and rows. A picture wider than the room
left on the line is fitted to that room — the pixels are untouched and the
rectangle is smaller, so the drawing of it is smaller too — because a picture
cut off at the right edge is a picture nobody can read.

The cursor comes to rest where the picture began, on the last row of it: one
line feed for every row but the last, which is where `xterm` and `foot` leave
it. A program that draws a picture and then writes beside it moves the cursor
itself — it knows the size of the picture it sent, and it steps past it with
`CSI {n} C` or puts the cursor back where it saved it — so a terminal that spent
a line of its own on the picture puts that text one line lower than the program
meant. `lss` lays out a listing exactly that way: room is made with line feeds,
the cursor goes back up, every thumbnail is drawn between a save and a restore,
and the caption is written to the right of it. What a program writes with no
cursor move of its own lands on the picture and takes those cells from it, which
is what those terminals do as well: a cell written into is a cell that is text
again, and the row it was part of stops being a whole row of the picture.

What is not read is `DECSET 8452`, which asks for the cursor to come to rest to
the right of the picture rather than at the start of it.

A picture keeps the cells it was placed in, and a page too narrow for it cuts
them: `Terminal::crop_pictures` empties the cells past the new width before the
grid is resized, so the reflow has nothing there to carry. A line too wide for
the page is otherwise wrapped onto the line below, which is what text wants —
nothing of it is lost — and what a picture gets out of it is a strip of itself
standing underneath and a page twice as tall as the one somebody printed. Only
the cells of a picture are emptied, so text beside one wraps the way it always
did.

A page made wide again gives the cells back (`Terminal::grow_pictures`): the
pixels never left — the store holds the whole of every picture — so what the cut
took is marks, and every row of a picture is grown from where it ends to where
its rectangle ends (`Picture::cells`) and no further. A picture is not scaled to
a window: wider than it was placed in, the page leaves it alone. Only empty
cells are written into and the growing stops at the first cell that is not,
because the room to the right of a picture is the page's and a program may have
written there while the window was narrow; a row whose first cell the cut took
has nothing to grow from and stays gone.

So what a page shows is as much of a picture as it holds, and never a picture of
another size. The one thing a picture is ever scaled to is the limit of the
settings, which is answered when it arrives.

Nothing is recomputed on a resize because the rows a picture stands on are lines
of the page: they were written when it arrived and carry whatever else went
through them — the caption beside a thumbnail is on those lines — so a picture
fitted again would have to move text that has already been printed, and a
terminal does not reflow its own output.

The words are the application's (`session::sixel_placeholder`,
`sixel.placeholder`) and the placing is the crate's
(`Terminal::set_sixel_placeholder`), because a line of text is the one thing a
library of this workspace may not hold and the cell it goes in is the one thing
only the emulation knows. A payload that declared no size is a question mark:
the standard lets the raster attribute be left out, and until the pixels are
read the size is not known.

Those cells are the picture rather than a note about one. Every one of them
carries its number — `Picture::cells` is the rectangle they make, so a renderer
can tell which part of a picture a cell shows when the first row of it has
scrolled off the page — and the right button on any of them opens the menu of
the picture (`ui::menu::image_items`), which holds what can be done with one and
nothing else — an entry about the selection or a transfer there would be an
entry nobody came for:

| entry | |
| --- | --- |
| save to a file | a PNG, under `PICTURE_FILE` |
| save and open it as … | the same file, handed to the program the desktop picks |
| copy | the picture on the clipboard of the platform, as pixels |
| fold into the line, unfold into the picture | which of the two the entry says follows the state it is looking at |

Folding is the one thing about a picture on the page that is a reader's answer
and not the program's (`Terminal::fold_picture`, `SnapshotPicture::folded`). A
folded picture is kept, stands in the cells it stood in and is drawn nowhere, so
what shows is the line that was written through those cells — a page of pictures
somebody is reading text in. Nothing else changes, so unfolding it is the
picture again.

PNG and not the payload, because a file is opened by whatever the desktop has
for its kind and what a desktop has for sixel is nothing; pixels and not the
payload on the clipboard, for the same reason. The plate that titles the menu is
the same line the picture shows, by the same call, because two ways of saying
what a picture is would be two ways of saying it differently.

The pixels are read when the picture arrives and not when somebody asks for
them. A picture of a screenful comes to milliseconds of reading — tens of them
in a build without optimisation — and that is a frame the window stands still
in; asked for on a press of the right button, it is a menu that takes its time
opening. So `zyt_term::decode` is called in `place_sixel`, the pixels are kept
beside the payload, and both the menu and the drawing are handed what is
already there. A payload the decoder refuses keeps nothing at all, and its line
is the text it is.

What the reading is held to is the picture limit (`Terminal::set_picture_limit`),
because a payload of four lines can declare a canvas of any size and the bytes
of it are four times that again. The raster attribute and the columns past the
right edge are cut before the decoder allocates anything for them.

A picture past the limit is shrunk to fit it, its proportions kept
(`zyt_term::decode`); one within it is kept as drawn and never enlarged. It is
not refused and not cut: the limit is a share of the window by default, so an
ordinary photograph is past it, and a cut picture keeps only a corner of what
was sent. The canvas a payload is read into is held to `zyt_term::MAX_CANVAS`,
or to the limit where that is larger, since a raster attribute asks for any size
it likes; what is past it is cut before the picture is shrunk.

A picture heavier than one may be — its payload and its shrunk pixels together
— is refused, and so is every picture where pictures are ignored. Its line says
which (`zyt_term::SixelRefusal`, `session::sixel_placeholder`), names no picture
and has no menu: there is nothing kept to save, and the reason is a setting the
reader can change.

A picture nobody draws can therefore be saved, handed to a viewer or copied:
drawing is one thing done with a picture and not what makes it one. Which of the
three a session gets is `docs/osc.md`; the rectangle of cells is taken for a
picture that is drawn, and a picture that is kept and not drawn stands as the
line alone, because a rectangle of empty cells is screen a device was not given.

How many pixels a picture is is what its raster attribute declared, cut to in
both directions: a program lays its page out against the size it asked for — a
thumbnail of eight columns and a caption stepped past nine — so a picture a row
or a column larger than that stands over what was written beside it. A payload
that declared nothing is as tall as the rows it painted: it is written in bands
of six, and the empty rows of the band it stopped in the middle of would be a
row of cells taken from the page. `foot` trims them for the same reason.

A program is told how large the page is before it draws on it. It asks in cells
(`CSI 18 t`) or in pixels — the text area (`CSI 14 t`) and one cell of it
(`CSI 16 t`) — and all three are answered from the same two numbers: the grid,
and `Terminal::set_cell_size`, which is the only place the size of a cell is
known at all. A program that draws pictures asks one of them first, because how
many pixels a cell is worth is how it decides what to send; one that reads no
answer takes it as a terminal with no size and draws nothing — `lss` says so in
as many words. Of the three the backend reads only `CSI 18 t`, so the sniffer
picks up `CSI 16 t` (`zyt_term::Sniffed::CellSize`) and the answer to `CSI 14 t`
is written where the backend asks for the size of the window.

A program is told what this terminal takes before it sends: `4` among the
primary device attributes, and the size and the colour registers in answer to
`XTSMGRAPHICS`. The size it is told is `Terminal::set_picture_limit`, which is
the size a larger picture is shrunk to, because an answer larger than the limit
would invite pixels that are sent only to be thrown away.

The page itself stands at the head of that block of the settings, in the three
ways a program is told it: the pixels of the text area, the cells of the page
and the pixels of one cell (`ui::settings::sixel`). A picture of so many pixels
is so many cells, so it is the arithmetic every program that draws does — and
the size of a cell is written down nowhere else, which is what somebody reading
a layout that came out wrong needs.

`Settings.sixel` is where that size comes from, what one picture may weigh and
what the pictures may hold together. The size is either a share of the window — worked out again every
frame (`App::apply_picture_limits`), since a window resized is a different
number of pixels and a program asking has to be told what is true now — or a
figure in pixels, which is what somebody means when the number matters. The
memory is a budget and not a count, the way the scrollback is: the oldest
pictures go as new ones arrive, and both halves of a picture count against it.

The weight is per picture, the payload and the shrunk pixels together, and it
is also how long a payload may grow while it arrives (`Terminal::set_picture_weight`).
It is separate from the budget because they answer two questions: whether one
picture is worth keeping at all, and how many of them stay.

The three are read against each other, so the page marks a size whose pixels
alone weigh more than one picture may (`SixelSettings::too_heavy`) and a budget
smaller than one picture (`SixelSettings::budget_short`), and corrects nothing:
which number was the mistake is not the page's to decide.

## Drawing a picture

The picture is a texture of the toolkit (`zyt_term_egui::picture`) and not part
of the mesh of the text, because one mesh carries one texture and the mesh of the
text carries the atlas of the font. It is uploaded the first time it is seen and
kept well past the frame it was last seen in: the pixels are held by the
terminal either way, and dropping the texture early would mean an upload every
time a page is scrolled over. The textures of a view are kept where its mesh is
kept, in `TerminalCache`, so dropping the cache of a view drops them with it.

What is drawn is one quad per run of cells. Every cell of a picture says which
cell of the picture it is (`zyt_term::ImagePart`), so what to draw in it is read
off the cell and nothing has to find the rest of the picture first — which is
what keeps a picture drawing when its first rows have scrolled off the page or a
narrower page has cut its right edge away. Neighbouring cells of one row are
then merged, so a picture eighty columns wide is one quad a row and not eighty.

A run is drawn only while it starts at the left edge of the picture
(`begins_a_row`): a run that does is that row of it, as much of it as the page
holds, and the cells the page wrapped onto the line below are cut off rather
than drawn there — a strip of a picture under the picture would be read as part
of it. A row cut off by the top or the bottom of the page draws like any other:
the page holds fewer of them, not narrower ones.

The page shows a picture or the line it left, never the two at once. Every cell
of a picture that is drawn draws no character of its own, the cut-off ones among
them (`TerminalView::covered`). The line is read where no picture is drawn, and
a page that drew both would be a picture with the words behind it showing
through: a thumbnail whose background a program cut away is transparent nearly
everywhere, and the line under it would be read through the picture. It is why
`zyt_term_egui::picture::Drawn` works the drawn runs out before the text of the
page is laid out rather than after — and why the kept mesh is told apart by the
pictures of the page as well, since folding one changes nothing else about it.

A cell of a picture nothing draws shows the line, which says what the picture
is: one the store has dropped, one the reader folded, one of a terminal that
keeps its pictures out of sight.

The texture is stretched over the cells it was placed in. The pixels of a
picture rarely divide into whole cells and the remainder is less than one cell
in each direction, so stretching it is a fraction of a cell either way where
leaving it would be a gap at the edge of every picture. It is magnified with no
filter and minified with one: a picture drawn larger than its pixels is one
whose pixels somebody wants to see, and one drawn smaller would come apart
without averaging.

What bounds the keeping is the budget and the weight
(`zyt_term::DEFAULT_PICTURE_BUDGET`, `zyt_term::DEFAULT_PICTURE_WEIGHT` until the
settings are applied): the first is how much the pictures of one terminal may
take together, the second how much one may weigh, which is also how long a
payload may be before the rest of it is thrown away. A payload the budget cannot
hold leaves the same line, naming no picture — the cells are then the text they
are, which is also what a reader gets for a picture dropped an hour ago.

A payload is read only as far as its head, which is where the raster attribute
stands, so a picture nobody draws costs what it holds and nothing else.
`experiments/sixel` is what a payload holds and what drawing one would cost.

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
