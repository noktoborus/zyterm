-- Files transferred over SHell: a file onto a device that runs nothing but a
-- shell, and back.
--
-- The far end runs no program of ours: every command is a shell script and
-- every answer is what it printed. `PROTOCOL.md` is the wire format and
-- `lib/fish/` is the library; this is the script that drives it, asks what the
-- transfer is to do and says how it went.

local fish = require("fish")

-- The sum of a file on this machine and on the device, compared.
local function compared(session, program, here, there)
    if not program then
        return
    end

    local ours = zyt.codec.digest(here, fish.settings.summing(program))
    local theirs = session:digest(program, there)
    if ours ~= theirs then
        error(
            string.format("%s is %s here and %s on the device", there, ours, theirs),
            0
        )
    end
end

-- One path under another, with the one slash between them.
local function joined(above, below)
    if above == "" or above == "." then
        return below
    end
    return (string.gsub(above, "/+$", "")) .. "/" .. below
end

return {

    send = function()
        local how = fish.settings.ask{
            title = "Shell transfer: onto the device",
            remote_label = "Directory on the device",
        }
        if not how then
            return
        end

        local items = zyt.fs.walk(zyt.target.paths())
        if #items == 0 then
            zyt.notice.error("there is nothing to send")
            return
        end

        local whole = fish.report.whole(items)
        local watch = fish.report.new{ whole = whole, verbose = how.verbose }
        local session = fish.session.new{
            mode = how.mode,
            chunk = how.chunk,
            size_check = how.size_check,
            timeout = how.timeout,
            watch = watch,
        }

        session:hello()
        local digest = fish.settings.digest_for(session, how.digest)

        for _, item in ipairs(items) do
            local there = joined(how.remote, item.relative)
            local above = fish.walk.above(there)
            if above and above ~= "" and above ~= "." then
                session:make_directory(above)
            end
            local carried = session:store(item.path, there)
            compared(session, digest, item.path, there)
            zyt.term.print(string.format("%s -> %s, %s", item.relative, there, zyt.codec.short_size(carried)))
        end

        zyt.progress.clear()
        zyt.notice.info(string.format(
            "%d of them, %s in all",
            #items,
            zyt.codec.short_size(whole)
        ))
    end,

    receive = function()
        local into = zyt.target.path()
        if not into then
            zyt.notice.error("there is nowhere to put what comes back")
            return
        end

        local how = fish.settings.ask{
            title = "Shell transfer: off the device",
            remote_label = "What to take off the device",
            with_all = true,
        }
        if not how then
            return
        end

        local watch = fish.report.new{ verbose = how.verbose }
        local session = fish.session.new{
            mode = how.mode,
            chunk = how.chunk,
            size_check = how.size_check,
            timeout = how.timeout,
            watch = watch,
        }

        session:hello()
        local digest = fish.settings.digest_for(session, how.digest)

        local items = fish.walk.remote(session, { how.remote })
        if #items == 0 then
            zyt.notice.error("the device has nothing there")
            return
        end

        local whole = fish.report.whole(items)
        local watching = fish.report.new{ whole = whole, verbose = how.verbose }
        session.watch = watching

        for _, item in ipairs(items) do
            local here = joined(into, item.relative)
            local above = fish.walk.above(here)
            if above and above ~= "" then
                zyt.fs.mkdir(above)
            end
            local carried = session:retrieve(item.path, here)
            compared(session, digest, here, item.path)
            zyt.term.write(string.format(
                "\27]8;;%s\27\\%s\27]8;;\27\\ %s\r\n",
                zyt.fs.url(here),
                item.relative,
                zyt.codec.short_size(carried)
            ))
        end

        zyt.progress.clear()
        zyt.notice.info(string.format(
            "%d of them, %s in all",
            #items,
            zyt.codec.short_size(whole)
        ))
    end,
}
