-- XModem, which carries neither the name of a file nor its length.
--
-- The name is therefore said on both ends: the device is told what to call
-- what it is about to receive, and a file coming back is written into the path
-- picked here. What arrives is padded to a whole sector of 128 bytes, because
-- the protocol has nowhere to say where the file ended.

local modem = require("modem")

return {

    send = function()
        local how = modem.ask{
            title = "XModem: onto the device",
            value = string.format("rx %s", zyt.shell.quote_posix(zyt.target.filename())),
        }
        if not how then
            return
        end

        modem.carry{
            remote = how.remote,
            command = "sx",
            args = { "-vv", zyt.target.path() },
        }
    end,

    receive = function()
        local how = modem.ask{
            title = "XModem: off the device",
            hint = "XModem carries no name and no length: what comes back is written into the path picked here, padded to a whole sector",
            value = "sx ",
            remote_hint = "What the device runs to send. It has to name the file there",
        }
        if not how then
            return
        end

        modem.carry{
            remote = how.remote,
            command = "rx",
            args = { "-vv", zyt.target.path() },
        }
    end,
}
