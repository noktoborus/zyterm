# The desktop

How a desktop reaches the program. The one name all of it is written under is
`main::APP_ID`, and the table of what carries it is in `ARCHITECTURE.md`.

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
