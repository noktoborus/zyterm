# plate-menu

A menu of wide plates for egui: it stands in the middle of the window, is walked
by the keys and is searched by typing.

## Scope

```rust,no_run
use plate_menu::{Beside, MenuItem, PlateMenu};

fn frame(context: &egui::Context, menu: &mut PlateMenu, asked: bool) {
    if asked {
        let items = vec![
            MenuItem::new("copy", "Copy").detail("ctrl+shift+c"),
            MenuItem::separator(),
            MenuItem::new("port", "Port").children(vec![
                MenuItem::new("usb0", "ttyUSB0"),
                MenuItem::new("usb1", "ttyUSB1"),
            ]),
        ];
        menu.open(items).expect("an entry can be chosen");
        menu.beside(Beside::Ignored); // after `open`, which resets it
    }
    menu.nothing_found("Nothing found"); // kept across openings
    menu.step_in_label("Open the entries inside");
    if let Some(chosen) = menu.show(context) {
        println!("{} shift={}", chosen.id, chosen.held.shift);
    }
}
```

### `PlateMenu`

| call | what it does |
| --- | --- |
| `new()`, `with_id(id)` | a closed menu; another id for a second one in the window |
| `max_plates(n)` | plates at once; default is what `HEIGHT_SHARE` of the window holds, less the two strips when not all fit |
| `open(items)` | opens on the first entry; resets `notice` and `beside` |
| `open_at(items, id)` | opens on that entry, in the level that carries it |
| `refill(items)` | new entries; the query, the level and the selection stay |
| `notice(text)` | the lines above everything |
| `nothing_found(text)` | the line of a query that finds nothing; kept across `open` |
| `step_in_label(text)` | what a screen reader calls the zone `›` of a split entry; kept across `open` |
| `beside(Closes \| Ignored)` | what a click beside the menu does |
| `show(context)` | draws a frame; `Some(Chosen)` on the frame an entry is chosen |
| `close()`, `is_open()`, `selected()` | |
| `holds(id)` | whether an entry of that id stands in the menu, at any level: is the menu standing still the one a caller opened |
| `layer_id()` | the layer, the same open or closed |

`Chosen` is the `id` and the modifiers `held` on that frame. They are reported
and never read: what `Shift+Enter` means is the caller's.

### `MenuItem`

| builder | |
| --- | --- |
| `new(id, label)` | an entry |
| `separator()` | a line |
| `icon(text)` | a mark before the label: what kind of thing the entry is; not searched, not read out |
| `detail(text)` | right of the label: a key, a value, a count |
| `hint(text)` | under the pointer |
| `search(text)` | more text the query matches, drawn nowhere |
| `searchable(false)` | no query finds it; reached through its parent |
| `full(text)` | the whole of a cut label, or what more the entry says, on a plate of its own |
| `enabled(false)` | drawn weak, not chosen |
| `children(items)` | a level below |
| `choosable(true)` | an entry with children is chosen; `Right` and its zone `›` step in |
| `opens_at(id)` | where the selection lands in the level below |

## The plates

![The parts of the menu][picture-parts]

| part | |
| --- | --- |
| notice | `notice(text)`, wrapped, above everything |
| query | while typing, under the notice, with its caret |
| `›` | the entry has children |
| detail | right of the label, weak |
| line | `separator()` |
| weak label | `enabled(false)` |
| strips | only while not all plates fit: a wave above and below, with the number of entries hidden on that side |
| width | the longest label, between `MIN_WIDTH` and `MAX_WIDTH` |

No row is ever shown in part. The window of plates follows the selection by
one row:

```text
rows   shown 3      Down        Down
 0     ┌ 0          0           0
 1     │ 1          ┌ 1         1
 2     └█2█         │ 2         ┌ 2
 3       3          └█3█        │ 3
 4       4            4         └█4█
```

`Up` above the first row and `Down` below the last wrap around. `PageUp` and
`PageDown` move by the plates shown and stop at the ends.

### The strips

```text
┌──────────────────────┐
│ ~~~~~~~~ 2 ~~~~~~~~~ │  two entries above the window
│ entry 2              │
│█entry 3             █│
│ entry 4              │
│ ~~~~~~~~ 4 ~~~~~~~~~ │  four below it
└──────────────────────┘
```

A strip is drawn on a side with hidden rows. A line between entries is not
counted. While not all rows fit, the room of both strips is kept, the side
with nothing hidden left empty, so the menu keeps its height while the window
walks. The wave is `WAVE_HEIGHT` high and `WAVE_LENGTH` long. A strip is no
button and nothing to accesskit.

### Dragging

A drag that starts on the menu moves the window, and the rows follow the
pointer: a row for every plate travelled, counted from the press. A finger
has no wheel, and a list longer than the window is walked this way on a touch
screen; a mouse drag does the same.

```text
 the pointer moves up by three plates

 rows   before      after
  0     ┌█0█          0
  1     │ 1           1
  2     │ 2           2
  3     │ 3           ┌█3█   the selection the window left lands
  4     │ 4           │ 4    on the row shown nearest to it
  5     └ 5           │ 5
  6       6           │ 6
  7       7           │ 7
  8       8           └ 8
```

| | |
| --- | --- |
| ends | the window stops at the first and the last row |
| release | chooses nothing: a drag is no click |
| beside the menu | a drag that starts there moves nothing and closes nothing |
| wheel | walks the selection, as `Down` and `Up` do |

![A drag of three plates][picture-drag]

## Levels

```text
 ┌───────────────┐   Right, Enter   ┌───────────────┐
 │█Port        ›█│ ───────────────► │ ‹ Port        │
 │ Theme       › │                  │█ttyUSB0      █│  or opens_at(id)
 │ Quit          │ ◄─────────────── │ ttyUSB1       │
 └───────────────┘ Left, Enter on ‹ └───────────────┘
```

| entry | `Enter` | `Right` | click |
| --- | --- | --- | --- |
| no children | chosen | nothing | chosen |
| children | steps in | steps in | steps in |
| children, `choosable(true)` | chosen | steps in | the label: chosen; the zone `›`: steps in |

A step out lands on the entry the level was stepped into from: `Port` above,
whichever way it was left.

![A level stepped into][picture-level]

### An entry that is chosen and has children

A pointer has no `Right`, so such a plate is split in two:

```text
┌──────────────────────────────┬─────┐
│ /dev/ttyUSB0          FTDI   │  ›  │
└──────────────────────────────┴─────┘
  click: chosen                 click: steps in
```

The zone is as wide as the plate is high. Each half is lit on its own under
the pointer, and each is a button to accesskit: the label, and the zone named
by `step_in_label(text)`, or by `›` without it.

![Entries with a zone][picture-split]

## Searching

```text
 ┌───────────────┐  type "usb"  ┌───────────────────────┐
 │ Port        › │ ───────────► │ usb|                  │  the caret
 │ Theme       › │              ├───────────────────────┤
 │ Quit          │ ◄─────────── │█Port · ttyUSB0       █│  parents · label,
 └───────────────┘  Esc, Left,  │ Port · ttyUSB1        │  what matched marked,
                    Backspace   └───────────────────────┘  best first
                    to empty
```

The whole tree is scored with `nucleo-matcher` over the label, the labels above
it and `search(text)`. `searchable(false)` keeps an entry out: in a flat list it
would not say which parent it acts on. The characters that matched are
drawn in the brightest text of the theme, in the label and in the parents; on
the selection the rest of the hit steps back by `MATCH_REST`. A match in
`search(text)` marks nothing, since that text is drawn nowhere.

While a query stands, the menu keeps the top edge it had when the query was
begun and gets shorter from the bottom, so the line typed in does not move. It
keeps its width too, and grows when a hit needs more. With the query gone it
stands in the middle again.

| before the query | the query `tty` |
| --- | --- |
| ![Before the query][picture-search-before] | ![The hits of a query][picture-search] |

A query nothing answers shows the line of `nothing_found(text)`, or an empty
plate without it. `Backspace` then cuts the query back to its longest
beginning that finds something, or to nothing: a word typed in the wrong
keyboard layout goes in one press.

| query | finds something from | `Backspace` gives |
| --- | --- | --- |
| `ttyxq` | `tty` | `tty` |
| `гыи` | no first letter | the empty query |
| `tty` | itself | `tt` |

![A query that finds nothing][picture-search-empty]

The query has a caret. Text and a paste go in at it, a paste of several lines
on one line.

## The plate of a whole

`full(text)` is shown for the selected entry. It is never chosen. It holds the
whole of a label cut to the plate, or what more the entry has to say than one
line holds; it wraps, and a newline in it is a line of its own.

| window width | where the plate stands | |
| --- | --- | --- |
| `2 × width + WHOLE_GAP` or more | beside, hanging from its top left corner; the menu moves left by half of it | ![beside][picture-whole-beside] |
| less | above, growing upwards, as wide as the menu | ![above][picture-whole-above] |

A plate hangs from a corner, so a whole of three lines and one of one line move
no entry.

Above, a menu too tall for both makes room in two steps:

```text
 ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
 │ ┌whole─────┐ │   │ ┌whole─────┐ │   │ ┌whole─────┐ │
 │ │          │ │   │ │          │ │   │ │          │ │
 │ ┌menu──────┐ │   │ └──────────┘ │   │ │          │ │
 │ │ entry 1  │ │   │ ┌menu──────┐ │   │ └──────────┘ │
 │ │ entry 2  │ │   │ │ entry 1  │ │   │ ┌menu──────┐ │
 │ │ entry 3  │ │   │ │ entry 2  │ │   │ │ entry 1  │ │
 │ └──────────┘ │   │ │ entry 3  │ │   │ │ ~~~ 2 ~~ │ │
 │              │   │ └──────────┘ │   │ └──────────┘ │
 └──────────────┘   └──────────────┘   └──────────────┘
   overlapping        1. the menu         2. then it gives
                      moves down          up plates, to one
```

![A menu shortened under the plate][picture-whole-above-shortened]

Room is made for the whole of the selection, and only while it has one. The
plate, the place of the menu and its length follow the selection, one step at a
time:

| on a short whole | one step down, on a tall one |
| --- | --- |
| ![A short whole][picture-whole-short] | ![A tall whole][picture-whole-tall] |

## Keys and pointer

| input | what it does |
| --- | --- |
| `Up`, `Down`, wheel | walk, wrapping |
| `PageUp`, `PageDown` | walk by the plates shown, stopping at the ends |
| `Home`, `End` | the first and the last entry; with a query, the caret to its start and end |
| `Right`, the zone `›` | step into the entry |
| `Left` | step out, or clear the query |
| `Enter`, click | choose, with whatever is held |
| `Esc` | clear the query; without one, close |
| text, a paste | into the query at the caret |
| `Backspace` | the character before the caret; with nothing found, back to what finds something |
| `Ctrl+Backspace` | clear the query |
| `Tab` | nothing |
| drag | move the window |
| click beside | `Beside::Closes` closes, `Beside::Ignored` nothing |
| pointer leaving | nothing |

These keys, the text, a paste and the wheel are taken out of the input, press
and release, so the view behind does not walk with the menu. `Tab` is taken so
the toolkit does not hand the keyboard to a widget behind. The query is not a
text field: `Left` and `Right` stay with the menu and typing needs no click.

Each plate is a button to accesskit, named by its label, selected while it is
the selection and disabled when it cannot be reached: a screen reader reads the
menu, and a test of a window that holds it finds an entry by its label. The
zone of a split entry is a second button, and the line of a query that finds
nothing is a label.

## Pictures

The pictures are tests: `tests/snapshots.rs` renders the menu with
`egui_kittest` and compares it with `img/<name>.png`. A change that moves a
pixel fails there, so the pictures show the menu as it is.

```sh
UPDATE_SNAPSHOTS=1 cargo test -p plate-menu --test snapshots   # draw them again
```

`build.rs` puts them into `cargo doc` as `data:` URLs.

## Examples

```sh
cargo run -p plate-menu --example flat     # a few entries and a separator
cargo run -p plate-menu --example nested   # entries inside entries, and a search
```

## Boundaries

No configuration, no text of its own, no knowledge of the application: every
label and identifier comes from the caller. The dependencies are `egui` and
`nucleo-matcher`. The widget blocks nothing behind it; a caller that wants that
uses `layer_id`.

## Errors

| error | when |
| --- | --- |
| `MenuError::Empty` | `open` or `refill` with nothing choosable; `refill` keeps the entries it had |
| `MenuError::Closed` | `refill` on a menu that is not open |

[picture-parts]: img/parts.png
[picture-level]: img/level.png
[picture-search]: img/search.png
[picture-whole-beside]: img/whole-beside.png
[picture-whole-above]: img/whole-above.png
[picture-whole-above-shortened]: img/whole-above-shortened.png
[picture-whole-short]: img/whole-short.png
[picture-whole-tall]: img/whole-tall.png
[picture-search-before]: img/search-before.png
[picture-search-empty]: img/search-empty.png
[picture-split]: img/split.png
[picture-drag]: img/drag.png
