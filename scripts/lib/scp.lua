-- What `scp -vv` says about a transfer, read back into where it stands.
--
-- `scp` draws its own progress only on a terminal, and a script runs it over
-- pipes. What it writes to stderr at `-vv` is enough to say the same thing:
-- which file starts and how long it is, and how many bytes the far end has
-- taken, which `ssh` says every time the window of the channel moves. The
-- lines are the ones of OpenSSH; `tests/data/scp/` holds them as a real
-- transfer wrote them.
--
-- | line | when | what it says |
-- | --- | --- | --- |
-- | `Executing: program …` | a connection starts; every remote path taken is one | the counting starts again |
-- | `Sending file modes: C0644 SIZE NAME` | the sender, old protocol (`-O`) | a file going, and its length |
-- | `Sink: C0644 SIZE NAME` | the receiver, old protocol | a file coming, and its length |
-- | `do_upload: upload local "PATH" …` | SFTP, a file going | its name; the length is this side's |
-- | `do_download: download remote "PATH" …` | SFTP, a file coming | its name; the length is the listing's |
-- | `channel N: rcvd adjust N` | the far end took that much of what was sent | bytes gone |
-- | `channel N: window N sent adjust N` | this side took that much of what came | bytes come |
-- | `Transferred: …` | a connection ends | the file of it is whole |
--
-- Both sides of the old protocol say their line, the far one through the
-- stderr of `ssh`: a file going is `Sending file modes` here and `Sink` there.
-- The line of this side is the one read. The window the channel opens with is
-- said as an adjust too, before any file: it is room and not bytes, and it is
-- not counted.

local modem = require("modem")

local scp = {}

-- Options of `scp` that take the next word, or the rest of their own, as a
-- value: a `v` in that value is no verbosity.
local VALUED = "cDFiJlPSoX"

-- How many times `-v` stands among the words given to `scp`, `-vv` and
-- `-Ov -v` counted alike. A letter that takes a value ends the word it is in,
-- and the word after it is that value.
function scp.verbosity(words)
    local count = 0
    local skip = false
    for _, word in ipairs(words) do
        if skip then
            skip = false
        elseif string.sub(word, 1, 1) == "-" and string.sub(word, 2, 2) ~= "-" then
            for at = 2, #word do
                local letter = string.sub(word, at, at)
                if letter == "v" then
                    count = count + 1
                elseif string.find(VALUED, letter, 1, true) then
                    skip = at == #word
                    break
                end
            end
        end
    end
    return count
end

-- Lines that are what `-vv` adds and nothing a person reads: the debugging of
-- `ssh` and of `scp`, the banner, and the lines a reader takes in.
local function noise(line)
    return string.match(line, "^debug%d: ")
        or string.match(line, "^scp: debug%d: ")
        or string.match(line, "^OpenSSH_")
        or string.match(line, "^Authenticated to ")
        or string.match(line, "^Bytes per second: ")
end

-- The last part of a path.
local function base_name(path)
    return string.match(path, "([^/\\]*)$") or path
end

-- A reader of what `scp` writes to stderr.
--
-- `how.direction` is `send` or `receive`, `how.files` the files of the run in
-- order, each `{ name = , size = }` — the size is what SFTP never says — and
-- `how.verbose` how many `-v` the run has (`scp.verbosity`). Below two the
-- lines say nothing about bytes, so the reader says nothing at all and keeps
-- no line back: no share is better than one that stands still.
--
-- `reader:take(line)` answers whether the line was taken in; a line not
-- taken is for the terminal. `on` is told what happens, `modem.report` unless
-- said otherwise.
function scp.reader(how)
    local on = how.on or modem.report
    local sizes = {}
    for _, file in ipairs(how.files or {}) do
        sizes[file.name] = file.size
    end

    local reader = {
        watching = (how.verbose or 0) >= 2,
        sending = how.direction == "send",
        files = #(how.files or {}),
        index = 0,
        counted = 0,
        from = 0,
        data = false,
        current = nil,
        meter = nil,
    }

    if reader.watching then
        on{ kind = "negotiating" }
    end

    local function done()
        local file = reader.current
        if not file then
            return
        end
        if file.size then
            reader.meter(file.size)
        end
        on{ kind = "file_done", name = file.name, taken = true }
        reader.current = nil
    end

    local function started(name, size)
        if reader.current and reader.current.name == name then
            return
        end
        done()
        reader.index = reader.index + 1
        reader.current = { name = name, size = size }
        reader.from = reader.counted
        reader.data = true
        reader.meter = modem.meter(size, on)
        on{
            kind = "file",
            direction = reader.sending and "send" or "receive",
            name = name,
            size = size,
            index = reader.index,
            files = reader.files > 0 and reader.files or nil,
        }
    end

    local function moved(bytes)
        if not reader.data or not reader.current then
            return
        end
        reader.counted = reader.counted + bytes
        local file = reader.current
        local went = reader.counted - reader.from
        if file.size then
            went = math.min(went, file.size)
        end
        reader.meter(went)
    end

    function reader:take(line)
        if not self.watching then
            return false
        end

        if string.match(line, "^Executing: program ") then
            self.counted, self.from, self.data = 0, 0, false
            return true
        end

        local size, name = string.match(line, "^Sending file modes: C%d+ (%d+) (.+)$")
        if size then
            if self.sending then
                started(name, tonumber(size))
            end
            return true
        end
        size, name = string.match(line, "^Sink: C%d+ (%d+) (.+)$")
        if size then
            if not self.sending then
                started(name, tonumber(size))
            end
            return true
        end

        local path = string.match(line, "do_upload: upload local \"(.-)\" to remote ")
            or string.match(line, "do_download: download remote \"(.-)\" to local ")
        if path then
            local file = base_name(path)
            started(file, sizes[file])
            return true
        end

        local adjust = self.sending and string.match(line, "channel %d+: rcvd adjust (%d+)")
            or not self.sending and string.match(line, "channel %d+: window %d+ sent adjust (%d+)")
        if adjust then
            moved(tonumber(adjust))
            return true
        end

        if string.match(line, "^Transferred: ") then
            done()
            self.data = false
            return true
        end

        return noise(line) ~= nil
    end

    -- Says the run is over: the file standing is whole, and the transfer is
    -- being closed.
    function reader:finish()
        if not self.watching then
            return
        end
        done()
        on{ kind = "finishing" }
    end

    return reader
end

return scp
