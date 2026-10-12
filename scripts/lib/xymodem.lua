-- XModem and YModem, spoken here.
--
-- Both are the same few rules: a receiver asks for blocks, `C` for ones closed
-- by a CRC and NAK for ones closed by a sum, and a sender answers with blocks
-- of 128 or 1024 bytes, numbered, each acknowledged before the next. YModem
-- puts a block 0 in front of every file, saying its name and length, and an
-- empty block 0 after the last. Written from Forsberg's description of the
-- two, so nothing of `lrzsz` is in it; `tests/xymodem.rs` holds it to `lrzsz`
-- on the device.
--
-- The bytes pass through the script. At the speed of a serial line that costs
-- nothing anybody notices, and it is what lets the transfer say how far it is
-- and be stopped with the device told so.

local xy = {}

local SOH, STX, EOT, ACK, NAK, CAN, SUB = 0x01, 0x02, 0x04, 0x06, 0x15, 0x18, 0x1A
local C = 0x43

-- How long an answer is waited for before the question is asked again.
local TIMEOUT = 10000

-- How many times one block or one question is tried.
local RETRIES = 10

-- How long the other end is given to start.
local START = 60000

-- How long a sender waits between two asks for the first block.
local ASK_EVERY = 3000

-- How many asks for blocks closed by a CRC come before an XModem receiver
-- settles for a sum.
local ASKS_FOR_CRC = 4

-- How long the bytes of one block may keep the receiver waiting.
local INSIDE_BLOCK = 1000

-- How long the line has to stay quiet for a byte asking to start to count as
-- one: the letter C in the echo of a command line is followed by the rest of
-- the line, and the C of a receiver by nothing.
local LONE = 100

-- How long the line has to stay quiet after something other than an answer to
-- the end of a transfer for the receiver to be taken for gone.
local GONE = 1000

-- How long the line has to stay quiet before a damaged block is asked again,
-- and the longest it is waited for.
local QUIET = 200
local QUIET_MOST = 3000

-- Eight cancels and eight backspaces: what a receiver or a sender takes for
-- "stop", and what rubs the cancels out again on a shell that was not in one.
local CANCEL = string.rep("\24", 8) .. string.rep("\8", 8)

-- True while a transfer is under way, so stopping the script tells the far end.
local speaking = false

-- The table of the CRC of XModem: CCITT, 0x1021, starting from zero.
local TABLE = {}
for at = 0, 255 do
    local crc = at << 8
    for _ = 1, 8 do
        if crc & 0x8000 ~= 0 then
            crc = ((crc << 1) ~ 0x1021) & 0xFFFF
        else
            crc = (crc << 1) & 0xFFFF
        end
    end
    TABLE[at] = crc
end

-- The CRC of a block.
function xy.crc16(data)
    local crc = 0
    for at = 1, #data do
        crc = ((crc << 8) & 0xFFFF) ~ TABLE[(crc >> 8) ~ string.byte(data, at)]
    end
    return crc
end

-- The sum of a block, which is what a receiver asking with NAK checks.
local function sum(data)
    local total = 0
    for at = 1, #data do
        total = total + string.byte(data, at)
    end
    return total & 0xFF
end

-- What stopping the script says to the far end. A script speaking either
-- protocol returns it as its `cleanup`.
function xy.cleanup()
    if speaking then
        speaking = false
        zyt.line.write(CANCEL)
    end
end

-- Ends the transfer and tells the far end.
local function fail(text)
    xy.cleanup()
    error(text, 0)
end

-- Ends the transfer the far end ended.
local function cancelled()
    speaking = false
    error("the device cancelled the transfer", 0)
end

-- The line, read a byte at a time out of what came in pieces.
local Line = {}
Line.__index = Line

local function line()
    return setmetatable({ piece = "", at = 1 }, Line)
end

-- The next byte, or nothing when none came in `timeout`.
function Line:byte(timeout)
    if self.at > #self.piece then
        local came = zyt.line.read(timeout)
        if not came then
            return nil
        end
        self.piece, self.at = came, 1
    end
    local byte = string.byte(self.piece, self.at)
    self.at = self.at + 1
    return byte
end

-- Puts the byte last read back.
function Line:unread()
    self.at = self.at - 1
end

-- The next `count` bytes, or nothing when the line stopped for `timeout`
-- before they all came.
function Line:take(count, timeout)
    local parts, have = {}, 0
    while have < count do
        if self.at > #self.piece then
            local came = zyt.line.read(timeout)
            if not came then
                return nil
            end
            self.piece, self.at = came, 1
        end
        local part = string.sub(self.piece, self.at, self.at + count - have - 1)
        self.at = self.at + #part
        have = have + #part
        parts[#parts + 1] = part
    end
    return table.concat(parts)
end

-- Drops what is on the line until it stays quiet: the rest of a damaged block,
-- which would otherwise be read as the start of the next one.
function Line:purge()
    self.piece, self.at = "", 1
    local waited = 0
    while waited < QUIET_MOST and zyt.line.read(QUIET) do
        waited = waited + QUIET
    end
end

-- Drops what already came and was not read, without waiting for more: an
-- answer to something sent before, which read as the answer to what is sent
-- next would put every answer after it one behind. One such shift at the end of
-- a file is an EOT sent once too often, and an EOT is a ctrl+d to the shell the
-- receiver has gone back to.
function Line:stale()
    self.piece, self.at = "", 1
    while zyt.line.waiting() > 0 do
        zyt.line.read(0)
    end
end

-- True when the next byte is a second cancel, which is how a far end stops.
function Line:cancels()
    return self:byte(INSIDE_BLOCK) == CAN
end

-- Says how far a transfer of `whole` bytes is, when there is a whole.
local function shown(whole)
    local last = -1
    return function(done)
        if not whole or whole == 0 then
            return
        end
        local share = math.floor(done * 100 / whole)
        if share ~= last then
            last = share
            zyt.progress.share(share)
        end
    end
end

-- One block, numbered, padded to its size with `pad` and closed with a CRC or
-- a sum.
local function block(number, data, size, crc, pad)
    data = data .. string.rep(pad, size - #data)
    local head = string.char(size == 1024 and STX or SOH, number & 0xFF, 255 - (number & 0xFF))
    local tail
    if crc then
        local check = xy.crc16(data)
        tail = string.char(check >> 8, check & 0xFF)
    else
        tail = string.char(sum(data))
    end
    return head .. data .. tail
end

-- What the receiver said to the last thing sent: ACK, NAK, or nothing in time.
-- What else it says — its C, the echo of a shell — is not an answer.
--
-- Unless `last` says that what was sent ends the transfer: then anything else
-- followed by quiet is the receiver gone and somebody else talking, and is
-- taken for an ACK. `lrzsz` empties its line as it leaves, which on a pseudo
-- terminal can take its last ACK with it; sending the end again would hand an
-- EOT, a ctrl+d, to the shell it went back to.
local function answer(line, last)
    local deadline = zyt.time.now_ms() + TIMEOUT
    local other = false
    while true do
        local left = deadline - zyt.time.now_ms()
        if left <= 0 then
            return nil
        end
        local byte = line:byte(other and math.min(left, GONE) or left)
        if byte == ACK or byte == NAK then
            return byte
        elseif byte == CAN and line:cancels() then
            cancelled()
        elseif byte == nil and other then
            return ACK
        elseif byte ~= nil and last then
            other = true
        end
    end
end

-- Sends a block until the receiver takes it.
local function deliver(line, packet, what, last)
    for _ = 1, RETRIES do
        line:stale()
        zyt.line.write(packet)
        if answer(line, last) == ACK then
            return
        end
    end
    fail(string.format("the device did not take %s", what))
end

-- Waits for the receiver to ask for the first block, and says whether it asked
-- for a CRC.
local function asked(line, wait)
    local deadline = zyt.time.now_ms() + wait
    while zyt.time.now_ms() < deadline do
        local byte = line:byte(500)
        if byte == C or byte == NAK then
            local next = line:byte(LONE)
            if next == nil or next == byte then
                return byte == C
            end
            line:unread()
        elseif byte == CAN and line:cancels() then
            cancelled()
        end
    end
    fail("the device never asked for the file")
end

-- Sends the bytes of an open file and its end. `large` lets blocks be 1024
-- bytes long, which only a receiver asking for a CRC takes.
local function send_body(line, file, size, crc, large, at)
    local number, sent = 1, 0
    while true do
        local length = 128
        if large and crc and size - sent > 128 then
            length = 1024
        end
        local data = file:read(length)
        if not data then
            break
        end
        deliver(line, block(number, data, length, crc, "\26"), string.format("block %d", number))
        number = number + 1
        sent = sent + #data
        at(sent)
    end

    for _ = 1, RETRIES do
        line:stale()
        zyt.line.write(string.char(EOT))
        if answer(line, true) == ACK then
            return
        end
    end
    fail("the device did not take the end of the file")
end

-- The last part of a path, which is what the far end is told a file is called.
local function base_name(path)
    return string.match(path, "([^/\\]*)$") or path
end

-- Sends one file with XModem. `large` says to send blocks of 1024 bytes when
-- the receiver asks for a CRC.
function xy.send_x(path, how)
    how = how or {}
    local file = zyt.fs.open(path)
    local size = file:size()
    local line = line()
    speaking = true
    zyt.progress.indeterminate()
    local crc = asked(line, START)
    send_body(line, file, size, crc, how.large, shown(size))
    file:close()
    speaking = false
    zyt.progress.clear()
end

-- Sends files with YModem, each named and measured in its block 0, and the
-- empty block 0 that ends the batch.
function xy.send_y(paths)
    local line = line()
    speaking = true
    for _, path in ipairs(paths) do
        local file = zyt.fs.open(path)
        local size = file:size()
        local stat = zyt.fs.stat(path) or {}
        zyt.progress.indeterminate()

        local header = base_name(path) .. "\0" .. tostring(size)
        if stat.modified then
            header = header .. string.format(" %o", stat.modified)
        end
        local length = #header < 128 and 128 or 1024
        if #header > 1024 then
            fail(string.format("the name of %s is too long for YModem", path))
        end
        local crc = asked(line, START)
        deliver(line, block(0, header, length, crc, "\0"), string.format("the name of %s", path))

        crc = asked(line, TIMEOUT)
        send_body(line, file, size, crc, true, shown(size))
        file:close()
    end

    local crc = asked(line, TIMEOUT)
    deliver(line, block(0, "", 128, crc, "\0"), "the end of the batch", true)
    speaking = false
    zyt.progress.clear()
end

-- The next header byte: a block, the end of a file, or nothing in `timeout`.
-- What else comes — the echo of the command, what the sender says about
-- itself — is passed over.
local function head_of(line, timeout)
    local deadline = zyt.time.now_ms() + timeout
    while true do
        local left = deadline - zyt.time.now_ms()
        if left <= 0 then
            return nil
        end
        local byte = line:byte(left)
        if byte == SOH or byte == STX or byte == EOT then
            return byte
        elseif byte == CAN and line:cancels() then
            cancelled()
        end
    end
end

-- The rest of a block whose head was read: its number and its bytes, or
-- nothing when it is damaged.
local function read_block(line, head, crc)
    local size = head == STX and 1024 or 128
    local rest = line:take(2 + size + (crc and 2 or 1), INSIDE_BLOCK)
    if not rest then
        return nil
    end
    local number, complement = string.byte(rest, 1, 2)
    if number + complement ~= 255 then
        return nil
    end
    local data = string.sub(rest, 3, 2 + size)
    if crc then
        local high, low = string.byte(rest, 3 + size, 4 + size)
        if xy.crc16(data) ~= (high << 8 | low) then
            return nil
        end
    elseif sum(data) ~= string.byte(rest, 3 + size) then
        return nil
    end
    return number, data
end

-- Asks for a first block until one comes: `C` for a CRC, or after a few of
-- them NAK for a sum when `settle` allows it. Answers with the head of what
-- came and whether it is closed with a CRC.
local function ask_first(line, wait, settle)
    local deadline = zyt.time.now_ms() + wait
    local asks = 0
    while zyt.time.now_ms() < deadline do
        local crc = not settle or asks < ASKS_FOR_CRC
        zyt.line.write(crc and "C" or string.char(NAK))
        asks = asks + 1
        local head = head_of(line, ASK_EVERY)
        if head then
            return head, crc
        end
    end
    fail("the device never sent anything")
end

-- Takes the blocks of one file, the head of the first already read, handing
-- the bytes of each to `write` and acknowledging it.
local function receive_body(line, head, crc, write)
    local expected, errors = 1, 0
    while true do
        if head == EOT then
            zyt.line.write(string.char(ACK))
            return
        end

        local number, data
        if head then
            number, data = read_block(line, head, crc)
        end
        if number == expected & 0xFF then
            write(data)
            expected = expected + 1
            errors = 0
            zyt.line.write(string.char(ACK))
        elseif number == (expected - 1) & 0xFF then
            -- The sender did not hear the last ACK and sent the block again.
            zyt.line.write(string.char(ACK))
        elseif number then
            fail(string.format("block %d came where %d was due", number, expected & 0xFF))
        else
            errors = errors + 1
            if errors > RETRIES then
                fail(string.format("block %d could not be read", expected))
            end
            line:purge()
            zyt.line.write(string.char(NAK))
        end
        head = head_of(line, TIMEOUT)
    end
end

-- Takes one file with XModem into `path`. What arrives is padded to a whole
-- block, because the protocol has nowhere to say where the file ended.
function xy.receive_x(path)
    local file = zyt.fs.open(path, "w")
    local line = line()
    local received = 0
    speaking = true
    zyt.progress.indeterminate()
    local head, crc = ask_first(line, START, true)
    receive_body(line, head, crc, function(data)
        file:write(data)
        received = received + #data
    end)
    file:close()
    speaking = false
    zyt.progress.clear()
    return received
end

-- The name a file offered under that name is written under: the last part of
-- it, so a name cannot reach out of the directory it is written into.
function xy.name_of(offered)
    local last = string.match(offered, "([^/\\]*)$") or ""
    last = string.match(last, "^%s*(.-)%s*$")
    if last == "" or last == "." or last == ".." then
        return nil
    end
    return last
end

-- Where a file of that name lands in `into`: under its own name, or beside a
-- file already there under a name of its own.
local function landing(into, name)
    local path = into .. "/" .. name
    if not zyt.fs.stat(path) then
        return path
    end
    for count = 1, 9999 do
        local beside = string.format("%s.%d", path, count)
        if not zyt.fs.stat(beside) then
            return beside
        end
    end
    return nil
end

-- What block 0 says: the name, and the length when the sender gave one.
local function header_of(data)
    local name, rest = string.match(data, "^([^%z]*)%z([^%z]*)")
    if not name then
        return nil
    end
    local size = tonumber(string.match(rest, "^%s*(%d+)") or "")
    return name, size
end

-- Takes files with YModem into the directory `into` until the sender says the
-- batch is over, and answers with what each was: `name`, `size`, `path`.
function xy.receive_y(into)
    local line = line()
    local files = {}
    speaking = true
    local wait = START
    while true do
        zyt.progress.indeterminate()
        local head, crc = ask_first(line, wait, false)
        wait = TIMEOUT * RETRIES

        local number, data
        if head == EOT then
            -- An end of a file whose ACK was lost: it is over already.
            zyt.line.write(string.char(ACK))
        else
            number, data = read_block(line, head, crc)
        end
        if number == 0 then
            local offered, size = header_of(data)
            zyt.line.write(string.char(ACK))
            if not offered or offered == "" then
                break
            end

            local name = xy.name_of(offered)
            local path = name and landing(into, name)
            local file = path and zyt.fs.open(path, "w")
            local received, at = 0, shown(size)
            head, crc = ask_first(line, TIMEOUT, false)
            receive_body(line, head, crc, function(bytes)
                if size then
                    bytes = string.sub(bytes, 1, size - received)
                end
                if file then
                    file:write(bytes)
                end
                received = received + #bytes
                at(received)
            end)
            if file then
                file:close()
            end
            files[#files + 1] = { name = offered, size = size, path = path, taken = path ~= nil }
        elseif head ~= EOT then
            line:purge()
        end
    end
    speaking = false
    zyt.progress.clear()
    return files
end

return xy
