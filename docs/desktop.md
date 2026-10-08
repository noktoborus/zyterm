# The desktop

How a desktop reaches the program: the icon the executable carries, and the
item a file manager shows for a folder. The one name all of it is written under
is `main::APP_ID`, and the table of what carries it is in `ARCHITECTURE.md`.

## The icon of the executable

There are two icons, and they are set in two places.

| icon | drawn by | set in |
| --- | --- | --- |
| the window | the window manager | `main::window_icon`, at run time |
| the executable | the shell of Windows | `build.rs`, at build time |

The window icon is enough on Linux, where the desktop finds the icon through
the desktop entry. Windows has no such entry: the shell draws a file with the
icon resource inside it, so an executable without one is drawn with the default
icon in a folder, on the taskbar and in the start menu — before the program has
run at all.

`build.rs` puts the resource in. It runs for a Windows target and returns at
once for any other, and a toolchain whose resource compiler is missing gets a
warning rather than a failed build.

`assets/icon.ico` is what it puts in: the icon drawn in `assets/icon.svg`,
rasterised at 16, 24, 32, 48, 64, 128 and 256 pixels. The sizes are in the file
because the shell picks one per place it draws, and one it has to scale itself
is the one that looks wrong.

## The item a file manager shows

A file manager offers a program for a folder when the program says it opens
one. `MimeType=inode/directory` in `assets/ru.styxheim.zyterm.desktop` is that
sentence, and it is what puts ZYTerm in the "Open With" list of every manager
that follows the desktop entry specification.

An item in the context menu itself is not in that specification, and the two
desktops answer for it differently.

| desktop | the item comes from |
| --- | --- |
| KDE | `assets/ru.styxheim.zyterm.open.desktop`, installed under `share/kio/servicemenus` |
| GNOME | `assets/nautilus/zyterm.py`, an extension run by `nautilus-python` |

The KDE file is read by KIO and needs nothing installed beside it. The GNOME
extension needs the `nautilus-python` package: GNOME Files has no file that adds
an item, so an extension is the only way there is one. A machine without that
package keeps the "Open With" list and loses nothing else.

Both of them, and the entry itself, start the program the same way: `zyterm %f`.
The manager puts the folder in place of `%f`, and the program reads it as the
`PATH` of its command line.

## Where a window starts

`PATH` is the directory of the process, set in `main::enter_path` before the
window opens. A file is read as the directory it stands in, so the item works on
either.

It is the directory of the process and not a setting, because that is already
what a console without a directory of its own is started in, and what a window
started from this one inherits. A console that names a directory is still
started there: naming it is asking for it.
