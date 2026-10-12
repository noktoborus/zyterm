-- YModem, which carries the name of the file and its length with its bytes.
--
-- The protocol is spoken here (`xymodem`), so nothing has to be installed on
-- this side. A file coming back lands under its own name in the directory
-- picked, beside one already there rather than over it.

local modem = require("modem")
local xy = require("xymodem")

return {

    send = function()
        local paths = zyt.target.paths()
        local how = modem.ask{
            title = "YModem: onto the device",
            value = "rb",
        }
        if not how then
            return
        end

        modem.tell(how.remote)
        xy.send_y(paths)
        zyt.notice.info(string.format("%d sent to the device", #paths))
    end,

    receive = function()
        local into = zyt.target.path()
        local how = modem.ask{
            title = "YModem: off the device",
            value = "sb ",
            remote_hint = "What the device runs to send. It has to name the files there",
        }
        if not how then
            return
        end

        modem.tell(how.remote)
        local files = xy.receive_y(into)
        local taken = 0
        for _, file in ipairs(files) do
            if file.taken then
                taken = taken + 1
            else
                zyt.notice.error(string.format("%s was not taken: the name cannot be written here", file.name))
            end
        end
        zyt.notice.info(string.format("%d of %d taken into %s", taken, #files, into))
    end,

    cleanup = xy.cleanup,
}
