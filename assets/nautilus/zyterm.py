"""The "Open with ZYTerm" item of the context menu of GNOME Files.

GNOME Files has no file of its own that adds an item to the top level of the
context menu: the mime type of the desktop entry puts the program in the "Open
With" list, and an item of its own is an extension and nothing else. This is
that extension, and `nautilus-python` is what runs it. A machine without that
package has the "Open With" list and not this item.

The program is started through its desktop entry rather than by name, so what
starts it is written in one place and the desktop tells its window apart by the
identity of the entry.
"""

import gi

gi.require_version("Nautilus", "4.0")

from gi.repository import Gio, GObject, Nautilus

DESKTOP_ENTRY = "ru.styxheim.zyterm.desktop"


def _directory(item):
    """The address of an item that is a directory, or nothing."""
    if item.get_uri_scheme() != "file" or not item.is_directory():
        return None
    return item.get_uri()


def _launch(uri):
    """Starts the program on one address through its desktop entry."""
    entry = Gio.DesktopAppInfo.new(DESKTOP_ENTRY)
    if entry is None:
        return
    entry.launch_uris([uri], None)


class OpenWithZYTerm(GObject.GObject, Nautilus.MenuProvider):
    """Adds the item to one selected folder and to the folder being shown."""

    def get_file_items(self, files):
        if len(files) != 1:
            return []
        uri = _directory(files[0])
        if uri is None:
            return []
        return [self._item("file", uri)]

    def get_background_items(self, folder):
        uri = _directory(folder)
        if uri is None:
            return []
        return [self._item("background", uri)]

    def _item(self, place, uri):
        item = Nautilus.MenuItem(
            name=f"OpenWithZYTerm::{place}",
            label="Open with ZYTerm",
            tip="Open a shell of ZYTerm in this folder",
        )
        item.connect("activate", lambda _item: _launch(uri))
        return item
