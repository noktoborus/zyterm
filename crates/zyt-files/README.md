# zyt-files

File operations of a terminal, run as cancellable tasks.

## Scope

- `path_of("file:///srv/two%20words/notes.txt")` gives the path an address
  names, escapes resolved. An address of another scheme, or of another host, is
  not a path here.
- `content_of(path)` says how a clipboard would carry the file — `Text`, `Image`
  or `Other` — from the media type `mime_guess` reads out of its name, which is
  all there is to go on without opening it. `application/json` and its like
  count as text.
- `TaskRunner::start(FileTask, notify)` runs one operation on a thread of its
  own and answers with its `TaskId`: `Move` (a rename, or a copy when it crosses
  a file system), `Trash`, `Delete`, `Read` with a size limit.
- `cancel(id)` and `cancel_all()` ask a task to stop where it is; `poll()`
  returns what happened since the last call and takes finished tasks off the
  list; `running()` reports each one with its bytes, its start and its guess of
  the time left.

## Guarantees

- A copy moves in 64 KiB chunks and looks at the cancel flag between them, so a
  cancel is answered within a chunk and the half written file is removed again.
- A deletion walks a directory and removes one entry at a time, so a cancel is
  answered between entries; what was already removed stays removed. A link is
  removed itself, never followed.
- The trash is one call to the desktop and cannot be interrupted: a cancel is
  answered before it starts and not after.
- `Read` asks for the size before it opens the file, so a file larger than the
  limit costs nothing but the question, and the limit is checked again while
  reading in case the file grew.
- A task that ends, well or badly, leaves the list of running tasks and is
  joined.

## Boundaries

No interface, no configuration, no text for the user: a caller names the tasks
in its own words. The trash of the desktop is the `trash` crate, the media types
are `mime_guess`.

## Errors

`FileError`: `Io` with the path, `Trash` with the error of the desktop,
`TooLarge` with the size and the limit, and `Cancelled`.
