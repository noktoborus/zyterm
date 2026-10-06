# The shell this crate sends

One file per command of the protocol, and one command per file: nothing here
is rendered into a hole of anything else. Each is built in with `include_str!`
into a `Command` (`../src/script.rs`), has its holes filled and is folded into
the one line the wire takes: comments and blank lines are dropped, the rest is
trimmed and joined with a space. Indentation is therefore free — write a script
the way it reads best — but every line has to end where the shell will take a
space next: `;`, `do`, `then`, `else`, `in`, `(` or `)`. A test holds you to it.

`PROTOCOL.md` says what each answer means. This says what each file is.

## The layout

```
scripts/
├── *.sh          asked the same way whichever mode carries the body
├── base64/       the mode's own: hello, retrieve, store, store-end
└── raw/          the mode's own: hello, retrieve, store
```

A mode is a whole catalogue of scripts, not a flag read halfway through a
command (`../src/catalog.rs`). `ModeCatalog::base64()` and `ModeCatalog::raw()`
each name one file per command: the ones in their own directory, and the shared
ones beside it.

| Field | base64 | raw |
|---|---|---|
| `hello` | `base64/hello.sh` | `raw/hello.sh` |
| `retrieve` | `base64/retrieve.sh` | `raw/retrieve.sh` |
| `store` | `base64/store.sh` | `raw/store.sh` |
| `body` | `Document`, closed by the client and ended with `base64/store-end.sh` | `Counted`, the device says when the line is ready |

Everything else — `pwd`, `canonical`, `kind`, `size`, `list`,
`make_directory`, `create`, `digest` and the three probes — is one file in the
directory above, because the question is the same whichever way a body travels.
A script that is the same for both modes belongs there and not in a copy under
each.

## The shared files

| File | What it asks the device | Answers with |
|---|---|---|
| `pwd.sh` | where it stands | the path |
| `canonical.sh` | what a directory really is, links resolved | the path, empty when there is none |
| `kind.sh` | what a path is | `D`, `F` or `O` |
| `size.sh` | how many bytes a file holds | the count on the reply line |
| `list.sh` | what a directory holds | `Pd`/`P-`, `S<size>`, `:<name>` per entry |
| `make-directory.sh` | to make a directory and those above it | a code only |
| `create.sh` | to empty a file before it is written | a code only |
| `digest.sh` | the sum of a file | `D<sum>` |
| `probe-md5sum.sh` | whether `md5sum` works | `Hmd5sum` or `Emd5sum` |
| `probe-sha1sum.sh` | whether `sha1sum` works | `Hsha1sum` or `Esha1sum` |
| `probe-sha256sum.sh` | whether `sha256sum` works | `Hsha256sum` or `Esha256sum` |

## The files of a mode

| File | What it asks the device | Answers with |
|---|---|---|
| `base64/hello.sh` | whether `base64`, `tail`, `head` and `wc` actually work | `H<command>` or `E<command>`, one per line |
| `raw/hello.sh` | whether `tail`, `head`, `wc` and `stty` actually work | the same |
| `base64/retrieve.sh` | one chunk, as base64 | lines of base64 |
| `raw/retrieve.sh` | one chunk, as the bytes themselves | the bytes, as many as were asked for |
| `base64/store.sh` | to take one chunk from a here-document | a code, once the client closes it |
| `base64/store-end.sh` | the reply that ends a body the client closed | a code |
| `raw/store.sh` | to take one chunk as the bytes themselves | ready, then a code |

A file travels in chunks. The size is asked for first, and a chunk being read
is a command of its own saying where it starts, so a transfer that stops says
where it stopped and the next command is worked out from the bytes that really
moved. A chunk being written is appended, because chunks go in the order they
stand in the file; the file is measured once, with `size`, when the last of
them has landed. Nothing reads or writes a byte at a time and nothing reads the
whole file per chunk: `tail` and `head` cut a chunk out in blocks, and a chunk
seeking to its offset with `dd bs=1` would cost the device a system call for
every byte it carries.

`size.sh` asks `ls -ln` first and falls back to `wc -c`, because the fifth
field of `ls -l` is only the size where `ls` is what it claims to be. It leaves
the reply line bare rather than saying zero when neither can say, and a chunk
cannot be cut from that, so the transfer stops there. `list.sh` does not use it
— one `wc` per entry would read a whole directory to draw a listing — so a
wrapped `ls` there costs the sizes in the listing and nothing else.

## Why a here-document carries base64 and `head -c` carries raw

Both end the body of a chunk being written, and which one works depends on what
the far end is.

A shell reading its commands from a pipe — an `ssh` session, a program driving
another program — reads them in blocks, and whatever it took would never reach
a `head -c` waiting behind it. A here-document is read by the shell itself,
which is the one reader that cannot be read past, so it is the one form that
crosses a pipe as readily as a console. base64 therefore uses it everywhere,
and a console echoes the body back, which is the price of one form that always
works.

Raw cannot travel in a here-document — a delimiter cannot be relied on inside
arbitrary bytes — so it counts the body out with `head -c`. That costs it
nothing it was not paying already: raw needs the line switched to binary, which
means a console, and a console is where `head -c` works.

## The holes

| Hole | Filled with |
|---|---|
| `{path}` | a path on the device, already quoted for its shell |
| `{offset}` | where in the file this chunk begins, counted from one, which is what `tail -c +N` counts in |
| `{count}` | how many bytes this chunk takes **on the wire** |
| `{program}` | the program that says a sum, with its own backslash |
| `{heredoc}` | the word that closes the here-document |

A chunk being written is rendered from the same values whichever mode carries
it: where it belongs, and how many bytes of the line its body takes. Raw uses
`{count}` to count the body out and base64 does not use it at all — that
difference is in the script, which is where the difference between the modes
belongs.

## The reply markers are not holes

A script writes its own:

| Written | Means |
|---|---|
| `\echo '##''# 000'` | done, nothing more to read |
| `\echo '##''# 001'` | the device is ready for what comes next |
| `\echo '##''# 100'` | the answer follows |
| `\echo '##''# 200'` | the answer ended |
| `\echo '##''# 500'` | refused |

It is written in two pieces because a console echoes back what it is sent, and
a script saying the marker plainly would come back looking exactly like the
answer it asks for. `'##''# 200'` is `### 200` to the shell and nothing of the
sort to anything reading the line. **Never write `###` in a script**, and never
let a value put there fill one; a test renders every script and fails if the
three characters appear.

The codes themselves are `wire::Reply` in Rust, and `wire::MARK` is the three
characters. Nothing in Rust fills a marker into a script any more.

## How long a line may be

A terminal in its usual mode holds one line and no more, and what does not fit
is dropped where it stands — no error, no gap.

| | characters |
| --- | --- |
| POSIX promises | 255 |
| Linux keeps | 4096 |
| the body of a file | 76, so it is safe anywhere |
| the listing here | about 340, past the promise |

So keep the commands as short as they can be, and never add a `{hole}` that
could carry something long. An offset and a count are numbers, which is why they
are safe to add.

## Two rules the tests hold you to

**Every command word carries a backslash.** `\ls`, `\dd`, `\echo`, `\read`. A
console is interactive, so it expands aliases, and a device whose profile wraps
a utility in one that adds a flag it does not know would fail the command
outright. The backslash is plain quoting: the alias is not looked for.

It does not reach past an alias to a shell *function* of the same name, so
nothing here should depend on a program where the shell can answer instead —
which is why `list.sh` takes its names from a glob and not from `ls`.

**Nothing is done to the shell that outlives the command.** A `cd` goes in a
subshell, and a `stty` is undone by the same line that set it — with
`\stty -raw echo min 1 time 0`, the plain inverse, and not with the settings kept in a
variable of the device: a variable would outlive the command that made it,
which is the thing this rule forbids. The protocol keeps no state on the far
end, and a command that failed halfway must not leave a console without an echo.

`raw/store.sh` is the one command the device reads the line in, and it sets
`min 0 time 100` with the raw mode: a read there returns nothing after ten
seconds of silence, so a client that was killed mid-chunk ends that `head`
instead of leaving it waiting for ever on a console nothing can reach.
