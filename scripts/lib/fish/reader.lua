-- Bytes of the line, with a deadline.
--
-- A console has no end of file, so a plain read on a device that stopped
-- answering would wait for ever. Every read is given a deadline instead, and a
-- device that says nothing within it is the one complaint this protocol cannot
-- do without.

local Reader = {}
Reader.__index = Reader

-- A reader waiting that many milliseconds for anything to arrive.
function Reader.new(timeout)
    return setmetatable({
        waiting = "",
        timeout = timeout or 30000,
        ended = false,
        pending = nil,
    }, Reader)
end

-- Takes in whatever is there, or says the device went quiet.
function Reader:fill()
    local data = zyt.line.read(self.timeout)
    if data == nil then
        if not zyt.line.is_open() then
            self.ended = true
            return
        end
        error(string.format("the device went quiet for %d ms", self.timeout), 0)
    end
    self.waiting = self.waiting .. data
end

-- One line, without the carriage return and the newline that ended it.
function Reader:line()
    if self.pending then
        local line = self.pending
        self.pending = nil
        return line
    end

    while true do
        local at = string.find(self.waiting, "\n", 1, true)
        if at then
            local line = string.sub(self.waiting, 1, at - 1)
            self.waiting = string.sub(self.waiting, at + 1)
            return (string.gsub(line, "[\r\n]+$", ""))
        end
        if self.ended then
            if self.waiting == "" then
                error("the device said nothing more", 0)
            end
            local line = self.waiting
            self.waiting = ""
            return (string.gsub(line, "%s+$", ""))
        end
        self:fill()
    end
end

-- Exactly that many bytes, however many lines they span.
function Reader:exact(count)
    while #self.waiting < count do
        if self.ended then
            break
        end
        self:fill()
    end

    local taken = string.sub(self.waiting, 1, count)
    self.waiting = string.sub(self.waiting, count + 1)
    return taken
end

-- Hands a line back, to be read again as the next one.
--
-- The lines of a body are read until one of them turns out to be the reply
-- that ends it, and that line still has to be answered by whoever asked.
function Reader:push_back(line)
    self.pending = line
end

return Reader
