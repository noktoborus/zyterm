-- What the transfer says while it runs.
--
-- The share of the whole goes to the bar; every command goes to the terminal
-- when the run was asked to say what it does, because a protocol that failed
-- is read by the command that failed and not by a number.

local report = {}

-- A set of calls a session is watched with.
function report.new(how)
    how = how or {}
    local whole = math.max(how.whole or 0, 0)
    local done = how.done or 0

    local watch = {}

    if how.verbose then
        watch.command = function(script)
            zyt.term.print("-> " .. script)
        end
    end

    watch.chunk = function(start, upto)
        if whole > 0 then
            local at = done + start
            zyt.progress.share(math.floor((at * 100) / whole))
        end
        if how.verbose then
            zyt.term.print(string.format("   %d..%d", start, upto))
        end
    end

    watch.carried = function(bytes)
        done = done + bytes
        if whole > 0 then
            zyt.progress.share(math.floor((done * 100) / whole))
        end
    end

    watch.done = function()
        return done
    end

    return watch
end

-- How much of a list of items there is to carry.
function report.whole(items)
    local whole = 0
    for _, item in ipairs(items) do
        whole = whole + (item.size or 0)
    end
    return whole
end

return report
