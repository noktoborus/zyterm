-- The reply line of the protocol.
--
-- Every answer of the device ends in a line carrying the marker and three
-- digits. The marker is looked for anywhere in the line and not at the start
-- of it: a shell writes its prompt without a newline after it, so the first
-- line of an answer arrives as `# ### 100` and an answer anchored to the start
-- of a line would never be seen.
--
-- That is safe only because nothing written to the device ever carries the
-- marker. A console echoes back what it is given, so the templates say it in
-- two pieces -- `'##''# 200'` -- which is the marker to the shell and nothing
-- of the sort to anything reading the line.

local wire = {}

wire.MARK = "###"

wire.DONE = 0
wire.READY = 1
wire.DATA = 100
wire.ENDED = 200
wire.FAILED = 500

-- The code a line carries and whatever followed it, or nothing when the line
-- is not a reply.
function wire.reply_of(line)
    local at = string.find(line, wire.MARK, 1, true)
    if not at then
        return nil
    end

    local rest = string.sub(line, at + #wire.MARK)
    local digits, text = string.match(rest, "^%s*(%d+)%s*(.-)%s*$")
    if not digits then
        return nil
    end
    return tonumber(digits), text or ""
end

-- The name of a code, for a complaint that has to say which it was.
function wire.name_of(code)
    local names = {
        [wire.DONE] = "done",
        [wire.READY] = "ready",
        [wire.DATA] = "an answer",
        [wire.ENDED] = "the end of an answer",
        [wire.FAILED] = "a refusal",
    }
    return names[code] or string.format("the code %d", code)
end

return wire
