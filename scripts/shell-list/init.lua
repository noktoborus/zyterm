-- What a directory of the device holds, without carrying anything.
--
-- It is the one command of this protocol that is not a transfer: nothing of
-- the line but a listing, which is what somebody who does not know what the
-- device holds asks for first.
--
-- The directory is not asked for. The device is asked where it stands and
-- that is the directory listed: the person at the console is already
-- somewhere, and a path typed into a form would be a second answer to it.

local fish = require("fish")

return {

    send = function()
        local how = zyt.ui.ask{
            title = "What the device holds",
            hint = "The directory the device stands in, whatever it is",
            fields = {
                {
                    name = "mode",
                    kind = "one_of",
                    label = "On the wire",
                    value = "base64",
                    options = { { "base64", "base64" }, { "raw", "raw, with stty" } },
                },
            },
        }
        if not how then
            return
        end

        local session = fish.session.new{ mode = how.mode }
        session:hello()

        local where = session:standing()
        local entries = session:list(where)
        if #entries == 0 then
            zyt.notice.info(string.format("%s holds nothing", where))
            return
        end

        for _, entry in ipairs(entries) do
            zyt.term.print(string.format(
                "%s %8s %s",
                (entry.kind == "directory") and "d" or (entry.kind == "file") and "-" or "?",
                zyt.codec.short_size(entry.size),
                entry.name
            ))
        end
        zyt.notice.info(string.format("%d entries in %s", #entries, where))
    end,
}
