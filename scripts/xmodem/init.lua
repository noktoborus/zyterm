-- XModem, which carries neither the name of a file nor its length.
--
-- The name is therefore said on both ends: the device is told what to call
-- what it is about to receive, and a file coming back is written into the path
-- picked here. What arrives is padded to a whole block, because the protocol
-- has nowhere to say where the file ended.
--
-- The protocol is spoken here (`xymodem`), so nothing has to be installed on
-- this side.

local modem = require("modem")
local xy = require("xymodem")

return {

    send = function()
        local path = zyt.target.path()
        local how = modem.ask{
            title = "XModem: onto the device",
            value = string.format("rx %s", zyt.shell.quote_posix(zyt.target.filename())),
            fields = {
                {
                    name = "large",
                    kind = "switch",
                    label = "1K blocks",
                    value = false,
                    hint = "XModem-1K: blocks of 1024 bytes, which not every receiver takes",
                },
            },
        }
        if not how then
            return
        end

        modem.tell(how.remote)
        xy.send_x(path, { large = how.large })
        zyt.notice.info(string.format("%s is on the device", path))
    end,

    receive = function()
        local path = zyt.target.path()
        local how = modem.ask{
            title = "XModem: off the device",
            hint = "XModem carries no name and no length: what comes back is written into the path picked here, padded to a whole block",
            value = "sx ",
            remote_hint = "What the device runs to send. It has to name the file there",
        }
        if not how then
            return
        end

        modem.tell(how.remote)
        local received = xy.receive_x(path)
        zyt.notice.info(string.format("%s took %s", path, zyt.codec.short_size(received)))
    end,

    cleanup = xy.cleanup,
}
