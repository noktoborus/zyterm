# keys

What a terminal sends when a key is pressed, byte for byte.

## Scope

- A key is not what reaches a program — bytes are. The tool reads its own
  standard input and writes down what came. It asks no library what key that
  was, because a library reading the same bytes cannot say which were missing.
Three panels:

| panel | what it shows |
| --- | --- |
| the last arrival | written out (`^C`, `ESC [ A`), with its bytes in hexadecimal and the names of the ones that have names |
| everything that came | newest first, with its time |
| the bytes worth counting | how often each has arrived |

- `^C` and `^U` lead that last list: a program is interrupted by the first, a
  line thrown away by the second, and both are a single byte a terminal sends
  only in raw mode. A grey line has never arrived.
- The bytes of one read are kept together, because that is how they were sent:
  an escape sequence is one arrival and not five.
- `^Q` twice in a row leaves. The first is written down like any other key, so
  the tool can show the key that leaves it.
- Nothing is negotiated: no keyboard protocol and no mode beyond the raw mode a
  terminal program needs, because what is wanted is what this terminal sends of
  its own accord. Bracketed paste is switched on so a paste can be told from
  typing.

## Running

```sh
cargo run -p keys
```

Run it inside this terminal and inside another one, and compare: the same key,
the same bytes, or not.
