# zyt-files

File operations of a terminal, run as cancellable tasks.

## Scope

| call | what it answers |
| --- | --- |
| `place_of(uri)` | where a link leads: `Here(path)` for a file of this machine, `Share` for `\\server\share` on Windows, `Away` for anything else |

A `file://` address is of this machine when it names no host, `localhost` or
the name of this machine, whole or by its first label either way, compared
without case; the name comes from `gethostname`. The escapes of its path are
resolved. On Windows `C:\x` and `C:/x` are paths here as well. A share is never
touched to learn what it holds: a server that does not answer holds the caller
for as long as Windows waits for it.
| `content_of(path)` | how a clipboard would carry the file — `Text`, `Image` or `Other` — from the media type `mime_guess` reads out of its name, which is all there is to go on without opening it; `application/json` and its like count as text |
| `TaskRunner::start(FileTask, notify)` | runs one operation on a thread of its own, and its `TaskId` |
| `cancel(id)`, `cancel_all()` | ask a task to stop where it is |
| `poll()` | what happened since the last call, taking finished tasks off the list |
| `running()` | each task with its bytes, its start and its guess of the time left |

| `FileTask` | |
| --- | --- |
| `Move` | a rename, or a copy when it crosses a file system |
| `Trash` | one call to the desktop |
| `Delete` | one entry at a time |
| `Read` | with a size limit |
| `Write` | of bytes the caller hands over |

## Guarantees

| task | a cancel is answered | what is left behind |
| --- | --- | --- |
| `Move` as a copy | within a 64 KiB chunk, the flag being looked at between them | nothing: the half written file is removed again |
| `Write` | the same, in the same chunks | nothing; an existing file is replaced, because where the bytes go is a question whoever picked the path has answered |
| `Delete` | between entries | what was already removed stays removed; a link is removed itself, never followed |
| `Trash` | before it starts and not after: one call to the desktop cannot be interrupted | — |
| `Read` | between chunks | the size is asked before the file is opened, so one larger than the limit costs nothing but the question, and the limit is checked again while reading in case the file grew |

A file holding the first half of what was asked for says nothing about which
half it is, which is why a cancelled copy or write leaves none of it. A task
that ends, well or badly, leaves the list of running tasks and is joined.

## Boundaries

No interface, no configuration, no text for the user: a caller names the tasks
in its own words. The trash of the desktop is the `trash` crate, the media types
are `mime_guess`.

## Errors

`FileError`: `Io` with the path, `Trash` with the error of the desktop,
`TooLarge` with the size and the limit, and `Cancelled`.
