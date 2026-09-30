# plate-menu

A menu of wide plates for egui.

## Scope

```rust
menu.open(items)?;                  // or open_at(items, id) to start on a value
menu.beside(Beside::Ignored);       // set after opening, which resets them
menu.notice("what happened");
if let Some(chosen) = menu.show(context) {
    act(&chosen.id, chosen.held);   // id, and the modifiers held on that frame
}
```

The modifiers are reported and never read: `Enter` and `Shift+Enter` both choose
the entry the selection stands on, a click and `Shift`+click both choose the one
under the pointer. What a second way of choosing the same entry means belongs to
the caller, which knows what the entry is.

| call | what it does |
| --- | --- |
| `open_at(items, id)` | opens on that entry, in the level that carries it; an unknown id opens like `open` |
| `refill(items)` | new entries, keeping the query, level and selection |
| `notice(text)` | the lines above everything, for a menu that is an answer |
| `beside(Closes \| Ignored)` | what a click beside the menu does |
| `shift(Auto \| Always)` | when room is kept for the plate beside the menu |
| `max_plates(n)` | how many plates at once; default is what the window fits |
| `layer_id()` | the layer, for a caller that wants nothing behind it reached |

`refill` is what a list read again while it stands needs — the devices of a
machine, once a second. The selection stays where it is while that entry exists,
a level that is gone falls back to the one above, and entries with nothing
choosable are refused, the menu keeping what it had.

## MenuItem

A tree of plain data: an identifier, a label, and

| field | meaning |
| --- | --- |
| `detail` | what stands right of the label: a key sequence, a value, a count |
| `hint` | the sentence the pointer uncovers |
| `search(text)` | more text the query is matched against and nothing draws |
| `full(text)` | the whole of a label that was cut, drawn on a plate of its own |
| `choosable(true)` | an entry with children is chosen; `Right` is then the way in |
| `opens_at(id)` | where the selection lands in the level below |
| `enabled(false)` | drawn, not reachable |
| `separator()` | a line |

`search` is how a port is found by the name of the device plugged into it: what
an entry is called and what it is known by are two questions.

The plate `full` draws hangs from the top right corner, so a whole of three
lines and a whole of one leave every entry where it was. Nothing on it can be
chosen. `shift(Always)` keeps the room for it while the selection walks, so a
level where only some entries carry one does not shuffle.

## Keys

| key | what it does |
| --- | --- |
| `Up` / `Down`, wheel | walk the plates, wrapping |
| `Right` | step into the entries of an entry |
| `Left` | step back out, or clear the query |
| `Enter` | choose, with whatever is held |
| `Esc` | close |
| letters, `Backspace` | the query |

An entry with children is stepped into rather than chosen, unless it says
`choosable`. Inside a level the first plate names the level it came from and
leads back.

All of those keys, the text and the wheel are taken out of the input, press and
release alike, so a view behind the menu does not walk along with it.

The query is not a text field, so `Left` and `Right` stay with the menu and
typing needs no click. Searching scores the whole tree with `nucleo-matcher`,
over the label, the labels above it and whatever `search` adds, and shows the
hits flat, best first.

Nothing scrolls: only the plates that fit are drawn, the window of plates walks
with the selection one row at a time, and a line below says which are on screen.
The menu stands in the middle of the window and never follows the pointer.

## Examples

```sh
cargo run -p plate-menu --example flat     # a few entries and a separator
cargo run -p plate-menu --example nested   # entries inside entries, and a search
```

## Boundaries

No configuration, no text of its own, no knowledge of the application: every
label and every identifier comes from the caller. The dependencies are `egui`
and `nucleo-matcher`.

## Errors

`MenuError::Empty` when a menu is opened or refilled with nothing choosable.
`MenuError::Closed` when entries are given to a menu that is not open.
