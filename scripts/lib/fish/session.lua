-- The conversation with the shell on the other end of the line.
--
-- Every command is a shell script that ends by echoing a reply line. The
-- device is told nothing about the protocol: it runs what it is given, and
-- what it prints is the answer. Everything that is not a reply line is noise
-- and is skipped -- the echo of a console, the prompt, the output of a command
-- that had nothing to say.

local catalog = require("fish.catalog")
local Reader = require("fish.reader")
local wire = require("fish.wire")

local Session = {}
Session.__index = Session

-- A conversation in that mode.
--
-- `chunk` is the size of a chunk on the wire, `timeout` how long the device
-- may say nothing, `size_check` whether a file that was written is measured
-- afterwards, and `watch` a table of calls told what happens.
function Session.new(how)
    how = how or {}
    local mode = how.mode or "base64"

    return setmetatable({
        catalog = catalog.new(mode),
        mode = mode,
        reader = Reader.new(how.timeout or 30000),
        chunk = math.max(how.chunk or catalog.DEFAULT_CHUNK, 1),
        size_check = how.size_check ~= false,
        watch = how.watch or {},
    }, Session)
end

-- How many bytes of a file one chunk carries.
function Session:slice()
    return catalog.slice(self.catalog, self.chunk)
end

-- Writes one line to the device.
--
-- A newline is what ends a line for a console as readily as the carriage
-- return a keyboard sends, and a pipe knows nothing else.
function Session:write_line(text)
    zyt.line.write_text(text .. "\n")
end

-- Sends one command of the catalogue with its holes filled.
function Session:send(name, holes)
    local script = zyt.script.fill(catalog.command(self.catalog, name), holes or {})
    if self.watch.command then
        self.watch.command(script)
    end
    self:write_line(script)
end

-- Reads until the device answers, skipping everything that is not a reply.
function Session:reply(command)
    while true do
        local line = self.reader:line()
        local code, text = wire.reply_of(line)
        if code then
            if code == wire.FAILED then
                error(string.format("%s was refused by the device", command), 0)
            end
            return code, text
        end
    end
end

-- Reads until the device answers and insists on one reply.
function Session:expect(command, wanted)
    local code = self:reply(command)
    if code == wanted then
        return
    end
    if wanted == wire.DONE and code == wire.ENDED then
        return
    end
    error(
        string.format(
            "%s answered %s where %s was due",
            command,
            wire.name_of(code),
            wire.name_of(wanted)
        ),
        0
    )
end

-- The lines between the two replies that frame them.
function Session:collect(command)
    local code = self:reply(command)
    if code == wire.ENDED or code == wire.DONE then
        return {}
    end
    if code ~= wire.DATA then
        error(
            string.format("%s answered %s where an answer was due", command, wire.name_of(code)),
            0
        )
    end

    local lines = {}
    while true do
        local line = self.reader:line()
        if wire.reply_of(line) then
            return lines
        end
        lines[#lines + 1] = line
    end
end

-- Sends a command and gathers the lines it frames.
function Session:ask(name, holes, command)
    self:send(name, holes)
    return self:collect(command)
end

-- The first program the device answered a probe with an `E` for.
local function missing(answer)
    for _, line in ipairs(answer) do
        local program = string.match(line, "^%s*E(%S+)")
        if program then
            return program
        end
    end
    return nil
end

-- Asks the device whether it has what this mode cannot do without.
--
-- Every tool is tried rather than asked after. An `E` ends the conversation
-- there: a transfer that cannot place a chunk is better refused before the
-- first byte than halfway through a file.
function Session:hello()
    local answer = self:ask("hello", {}, "#VER")
    local absent = missing(answer)
    if absent then
        error(string.format("the device has no working %s", absent), 0)
    end
end

-- Whether the device can say that sum of a file.
function Session:offers_digest(program)
    if not catalog.probe(self.catalog, program) then
        return false
    end
    return missing(self:ask("probe_" .. program, {}, "#PROBE")) == nil
end

-- Where the device stands.
function Session:pwd()
    local answer = self:ask("pwd", {}, "#PWD")
    return answer[1] or ""
end

-- Where the device stands, insisting on an answer.
--
-- A transfer works below this directory and asks for no path of its own: the
-- directory the person at the console is working in is what they mean by
-- "here", and a device that will not say where it stands is one nothing can be
-- carried to by that name.
function Session:standing()
    local where = self:pwd()
    if where == "" then
        error("the device would not say where it stands", 0)
    end
    return where
end

-- The path a directory really is, with every symbolic link resolved.
function Session:canonical(directory)
    local answer = self:ask("canonical", { path = zyt.shell.quote_posix(directory) }, "#CANON")
    return answer[1] or ""
end

-- What a path of the device is: `file`, `directory` or `other`.
function Session:kind(path)
    local answer = self:ask("kind", { path = zyt.shell.quote_posix(path) }, "#KIND")
    local said = answer[1]
    if said == "D" then
        return "directory"
    elseif said == "F" then
        return "file"
    end
    return "other"
end

-- How many bytes a file of the device holds.
--
-- A device that cannot say leaves the reply line bare, and that is the end of
-- it: a body travels in chunks, and a chunk is cut from a size.
function Session:size(path)
    self:send("size", { path = zyt.shell.quote_posix(path) })
    local code, text = self:reply("#SIZE")
    if code ~= wire.DATA then
        error(
            string.format("#SIZE answered %s where an answer was due", wire.name_of(code)),
            0
        )
    end
    local size = tonumber(string.match(text or "", "^%s*(%d+)%s*$"))
    self:expect("#SIZE", wire.ENDED)
    if not size then
        error(string.format("nothing on the device can say how big %s is", path), 0)
    end
    return size
end

-- What the device says the sum of a file there is.
function Session:digest(program, path)
    local answer = self:ask("digest", {
        program = "\\" .. program,
        path = zyt.shell.quote_posix(path),
    }, "#SUM")

    for _, line in ipairs(answer) do
        local sum = string.match(line, "^%s*D(%S+)")
        if sum then
            return sum
        end
    end
    error(string.format("the device would not say the %s of %s", program, path), 0)
end

-- What a directory of the device holds.
--
-- One entry is three lines: what it is, how big it is, and its name, which
-- closes the entry. The names come from the shell and not from a program.
function Session:list(directory)
    local lines = self:ask("list", { path = zyt.shell.quote_posix(directory) }, "#LIST")
    local entries = {}
    local kind, size = "other", 0

    for _, line in ipairs(lines) do
        local mark, rest = string.sub(line, 1, 1), string.sub(line, 2)
        if mark == "P" then
            local letter = string.sub(rest, 1, 1)
            kind = (letter == "d") and "directory" or (letter == "-") and "file" or "other"
        elseif mark == "S" then
            size = tonumber(string.match(rest, "^%s*(%d+)") or "0") or 0
        elseif mark == ":" then
            if rest ~= "" and rest ~= "." and rest ~= ".." then
                entries[#entries + 1] = { name = rest, kind = kind, size = size }
            end
            kind, size = "other", 0
        end
    end
    return entries
end

-- Makes a directory on the device, and every directory above it.
function Session:make_directory(directory)
    self:send("make_directory", { path = zyt.shell.quote_posix(directory) })
    self:expect("#MKD", wire.DONE)
end

-- Makes a file of no bytes, throwing away what stood there.
function Session:create(path)
    self:send("create", { path = zyt.shell.quote_posix(path) })
    self:expect("#CREA", wire.DONE)
end

-- Reads lines of base64 until the reply that ends the chunk.
function Session:read_base64(file)
    local done = 0
    while true do
        local line = self.reader:line()
        if wire.reply_of(line) then
            self.reader:push_back(line)
            return done
        end
        if string.match(line, "%S") then
            local bytes = zyt.codec.b64_decode(line)
            file:write(bytes)
            done = done + #bytes
        end
    end
end

-- Reads the bytes themselves, exactly as many as were asked for.
function Session:read_raw(file, count)
    local bytes = self.reader:exact(count)
    file:write(bytes)
    return #bytes
end

-- Reads a file of the device into a file of this machine, chunk by chunk.
--
-- The size is asked for first, because a chunk is cut from it. Every chunk is
-- a command of its own, so a transfer that stops says where it stopped, and
-- the next command is decided by the bytes that really came rather than by the
-- ones that were asked for.
function Session:retrieve(path, into)
    local size = self:size(path)
    local quoted = zyt.shell.quote_posix(path)
    local slice = self:slice()
    local file = zyt.fs.open(into, "w")
    local start = 0

    local ok, said = pcall(function()
        while start < size do
            local count = math.min(slice, size - start)
            self:send("retrieve", {
                path = quoted,
                offset = tostring(start + 1),
                count = tostring(count),
            })
            if self.watch.chunk then
                self.watch.chunk(start, start + count, size)
            end
            self:expect("#RETR", wire.DATA)

            local got
            if self.mode == "raw" then
                got = self:read_raw(file, count)
            else
                got = self:read_base64(file)
            end
            self:expect("#RETR", wire.ENDED)

            if got == 0 then
                error(
                    string.format("%s stopped after %d of %d bytes", path, start, size),
                    0
                )
            end
            start = start + got
            if self.watch.carried then
                self.watch.carried(got)
            end
        end
    end)

    file:close()
    if not ok then
        error(said, 0)
    end
    return start
end

-- Writes one chunk, whatever the body of one is made of in this mode.
function Session:write_chunk(quoted, bytes, start)
    local body = catalog.body_of(self.catalog, bytes)
    local delimiter = (self.catalog.body == "document") and catalog.HEREDOC or ""

    self:send("store", {
        path = quoted,
        count = tostring(#body),
        heredoc = delimiter,
    })
    if self.watch.chunk then
        self.watch.chunk(start, start + #bytes)
    end

    if self.catalog.body == "counted" then
        self:expect("#STOR", wire.READY)
        zyt.line.write(body)
    else
        zyt.line.write(body)
        self:write_line(delimiter)
        self:send("store_end", {})
    end
    self:expect("#STOR", wire.ENDED)
end

-- Writes a file of this machine onto the device, chunk by chunk.
--
-- The file is emptied first and every chunk is appended. When the last chunk
-- has landed the device is asked how long the file is: a chunk that never
-- arrived is a file of another length, and asking once costs one command
-- instead of reading the whole file over again after every chunk.
function Session:store(from, path)
    self:create(path)
    local quoted = zyt.shell.quote_posix(path)
    local slice = self:slice()
    local start = 0

    while true do
        local bytes = zyt.codec.slice(from, start + 1, slice)
        if #bytes == 0 then
            break
        end
        self:write_chunk(quoted, bytes, start)
        start = start + #bytes
        if self.watch.carried then
            self.watch.carried(#bytes)
        end
    end

    if self.size_check then
        local there = self:size(path)
        if there ~= start then
            error(
                string.format(
                    "%s is %d bytes on the device and %d were written",
                    path,
                    there,
                    start
                ),
                0
            )
        end
    end
    return start
end

return Session
