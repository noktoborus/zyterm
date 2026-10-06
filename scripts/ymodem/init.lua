-- YModem, which carries the name of the file as well as its bytes.

local modem = require("modem")

return {

    send = function()
        local how = modem.ask{
            title = "YModem: onto the device",
            value = "rb",
        }
        if not how then
            return
        end

        modem.carry{
            remote = how.remote,
            command = "sb",
            args = { "-vv", zyt.target.path() },
        }
    end,

    receive = function()
        local how = modem.ask{
            title = "YModem: off the device",
            value = "sb ",
            remote_hint = "What the device runs to send. It has to name the file there",
        }
        if not how then
            return
        end

        modem.carry{
            remote = how.remote,
            command = "rb",
            args = { "-vv" },
            cwd = zyt.target.path(),
        }
    end,
}
