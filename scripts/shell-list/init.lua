-- What a directory of the device holds, without carrying anything.
--
-- It is the one command of this protocol that is not a transfer: nothing of
-- the line but a listing, which is what somebody who does not know where a
-- file stands asks for first.

local fish = require("fish")

return {

    send = function()
        local how = zyt.ui.ask{
            title = "What the device holds",
            fields = {
                {
                    name = "remote",
                    kind = "text",
                    label = "Directory on the device",
                    value = ".",
                    required = true,
                },
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

        local entries = session:list(how.remote)
        if #entries == 0 then
            zyt.notice.info("there is nothing there")
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
        zyt.notice.info(string.format("%d entries", #entries))
    end,
}
