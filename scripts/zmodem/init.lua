-- ZModem, the one most devices have.
--
-- The device is told first and the program here starts once the device has
-- taken the command, whichever way the file goes.
--
-- Sending is the one case that works with the device told nothing: `sz` writes
-- `rz` down the line itself, which is what makes a shell on the far end start
-- the receiver. The switch is therefore worth turning off for a device that
-- answers that by itself, and worth leaving on for one that does not.

local modem = require("modem")

return {

    send = function()
        local how = modem.ask{
            title = "ZModem: onto the device",
            value = "rz -y",
        }
        if not how then
            return
        end

        modem.carry{
            remote = how.remote,
            command = "sz",
            args = { "-vv", "-b", zyt.target.path() },
        }
    end,

    receive = function()
        local how = modem.ask{
            title = "ZModem: off the device",
            value = "sz -b ",
            remote_hint = "What the device runs to send. It has to name the file there",
        }
        if not how then
            return
        end

        modem.carry{
            remote = how.remote,
            command = "rz",
            args = { "-vv", "-b", "-E" },
            cwd = zyt.target.path(),
        }
    end,
}
