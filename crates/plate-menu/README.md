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
    if let Some(chosen) = menu.show(context) {
        println!("{} shift={}", chosen.id, chosen.held.shift);
    }
}
```

### `PlateMenu`

| call | what it does |
| --- | --- |
| `new()`, `with_id(id)` | a closed menu; another id for a second one in the window |
| `max_plates(n)` | plates at once; default is what `HEIGHT_SHARE` of the window holds |
| `open(items)` | opens on the first entry; resets `notice` and `beside` |
| `open_at(items, id)` | opens on that entry, in the level that carries it |
| `refill(items)` | new entries; the query, the level and the selection stay |
| `notice(text)` | the lines above everything |
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
| `detail(text)` | right of the label: a key, a value, a count |
| `hint(text)` | under the pointer |
| `search(text)` | more text the query matches, drawn nowhere |
| `searchable(false)` | no query finds it; reached through its parent |
| `full(text)` | the whole of a cut label, or what more the entry says, on a plate of its own |
| `enabled(false)` | drawn weak, not chosen |
| `children(items)` | a level below |
| `choosable(true)` | an entry with children is chosen; `Right` steps in |
| `opens_at(id)` | where the selection lands in the level below |

## The plates

![The parts of the menu][picture-parts]

| part | |
| --- | --- |
| notice | `notice(text)`, wrapped, above everything |
| query | while typing, under the notice |
| `›` | the entry has children |
| detail | right of the label, weak |
| line | `separator()` |
| weak label | `enabled(false)` |
| `1–7 / 9` | only while not all plates fit |
| width | the longest label, between `MIN_WIDTH` and `MAX_WIDTH` |

Nothing scrolls. The window of plates follows the selection by one row:

```text
rows   shown 3      Down        Down
 0     ┌ 0          0           0
 1     │ 1          ┌ 1         1
 2     └█2█         │ 2         ┌ 2
 3       3          └█3█        │ 3
 4       4            4         └█4█
```

`Up` above the first row and `Down` below the last wrap around.

## Levels

```text
 ┌───────────────┐   Right, Enter   ┌───────────────┐
 │█Port        ›█│ ───────────────► │ ‹ Port        │
 │ Theme       › │                  │█ttyUSB0      █│  or opens_at(id)
 │ Quit          │ ◄─────────────── │ ttyUSB1       │
 └───────────────┘ Left, Enter on ‹ └───────────────┘
```

| entry | `Enter` | `Right` |
| --- | --- | --- |
| no children | chosen | nothing |
| children | steps in | steps in |
| children, `choosable(true)` | chosen | steps in |

![A level stepped into][picture-level]

## Searching

```text
 ┌───────────────┐  type "usb"  ┌───────────────────────┐
 │ Port        › │ ───────────► │ usb                   │
 │ Theme       › │              ├───────────────────────┤
 │ Quit          │ ◄─────────── │█Port · ttyUSB0       █│  parents · label
 └───────────────┘  Backspace   │ Port · ttyUSB1        │  best first
                    to empty,   └───────────────────────┘
                    or Left
```

![The hits of a query][picture-search]

The whole tree is scored with `nucleo-matcher` over the label, the labels above
it and `search(text)`. `searchable(false)` keeps an entry out: in a flat list it
would not say which parent it acts on.

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
 │ └──────────┘ │   │ │ entry 3  │ │   │ │ 1–1 / 3  │ │
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
| `Right` | step into the entry |
| `Left` | step out, or clear the query |
| `Enter`, click | choose, with whatever is held |
| `Esc` | close |
| text, `Backspace` | the query |
| click beside | `Beside::Closes` closes, `Beside::Ignored` nothing |
| pointer leaving | nothing |

These keys, the text and the wheel are taken out of the input, press and
release, so the view behind does not walk with the menu. The query is not a
text field: `Left` and `Right` stay with the menu and typing needs no click.

Each plate is a button to accesskit, named by its label, selected while it is
the selection and disabled when it cannot be reached: a screen reader reads the
menu, and a test of a window that holds it finds an entry by its label.

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
