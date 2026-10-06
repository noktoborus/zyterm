-- Files transferred over SHell: a file onto a device that runs nothing but a
-- shell, and back.
--
-- `PROTOCOL.md` beside the scripts is the wire format. This is the library a
-- script of this protocol is written with: the conversation, the templates of
-- its commands, the walk of the device and the dialog that settles how a
-- transfer runs.

local fish = {
    catalog = require("fish.catalog"),
    reader = require("fish.reader"),
    report = require("fish.report"),
    session = require("fish.session"),
    settings = require("fish.settings"),
    walk = require("fish.walk"),
    wire = require("fish.wire"),
}

return fish
