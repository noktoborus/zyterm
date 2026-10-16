-- The shape every modem transfer has.
--
-- A modem program is not talked to: it is started, handed the line and left to
-- it. The device is told to run its counterpart first, and the program here
-- starts a moment later, whichever way the file is going. The order matters
-- and it is not the obvious one: the program here begins its handshake the
-- moment it starts, and a handshake that arrives while the shell of the device
-- is still waiting for a command line is read as part of that command line.
--
-- Nothing of the file passes through this script: `attach_to_line` hands the
-- pipes of the program to the line on the side of the host, so a transfer
-- costs what the line costs and nothing per byte.

local modem = {}

-- How long the far end is given to start before the program here does.
--
-- It is an upper bound and not a wait: the device usually says something as
-- soon as it has taken the command — a console echoes it back — and that is
-- the signal the program here may start. The bound is what a line that echoes
-- nothing falls back to.
modem.DELAY = 2000

-- How long the device is given after it first says something.
--
-- The echo of a command line arrives while the shell is still reading it, so
-- what the echo says is "it is being read", not "the program is running".
modem.SETTLE = 300

-- The longest the bytes of a transfer go unsaid while they move slower than
-- one share a step.
local BYTES_EVERY = 200

-- The file a transfer is on, and whether the last thing said was a retry, so
-- the stage goes back to the file once bytes move again.
local current = nil
local retrying = false

-- The stage of a file going or coming.
local function file_stage(event)
    zyt.progress.stage{
        kind = event.direction == "send" and "sending" or "receiving",
        name = event.name,
        file = event.index,
        files = event.files,
    }
end

-- Says what a transfer is doing, from the events every protocol here sends:
-- `zyt.zmodem` calls it as its `on`, `xymodem` and `modem.carry` call it
-- with the same events. One function, so the three say it the same way.
--
-- | `kind` | fields |
-- | --- | --- |
-- | `negotiating` | |
-- | `file` | `direction` (`send` or `receive`), `name`, `size`, `index`, `files` |
-- | `bytes` | `done`, `whole` |
-- | `file_done` | `name`, `taken` |
-- | `retrying` | `count` |
-- | `finishing` | |
function modem.report(event)
    local kind = event.kind
    if kind == "negotiating" then
        current, retrying = nil, false
        zyt.progress.stage{ kind = "negotiating" }
        zyt.progress.indeterminate()
    elseif kind == "file" then
        current, retrying = event, false
        file_stage(event)
        zyt.progress.bytes(0, event.size)
    elseif kind == "bytes" then
        if retrying and current then
            retrying = false
            file_stage(current)
        end
        zyt.progress.bytes(event.done, event.whole)
    elseif kind == "retrying" then
        retrying = true
        zyt.progress.stage{
            kind = "retrying",
            count = event.count,
            name = current and current.name,
        }
    elseif kind == "finishing" then
        current, retrying = nil, false
        zyt.progress.stage{ kind = "finishing" }
    end
end

-- A function saying the bytes of a file of `whole` as they go: when the share
-- moves, when the last one went, and otherwise every `BYTES_EVERY`. `on` is
-- what is told, `modem.report` unless said otherwise.
function modem.meter(whole, on)
    on = on or modem.report
    local last_share, last_at = nil, nil
    return function(done)
        local share = (whole and whole > 0) and math.floor(done * 100 / whole) or nil
        local now = zyt.time.now_ms()
        if last_at and share == last_share and now - last_at < BYTES_EVERY and done ~= whole then
            return
        end
        last_share, last_at = share, now
        on{ kind = "bytes", done = done, whole = whole }
    end
end

-- What `-vv` writes about where a transfer stands, when it writes anything:
-- `Bytes Sent: 4096/ 8192`, `Bytes Received` the other way round, and the
-- total missing when the program does not know it.
local function bytes_of(said)
    local done, whole = string.match(said, "Bytes%s+%a+:%s*(%d+)/%s*(%d+)")
    if not done then
        done = string.match(said, "Bytes%s+%a+:%s*(%d+)")
    end
    if not done then
        return nil
    end
    return tonumber(done), tonumber(whole)
end

-- The file `-vv` says it starts on: `Sending: name` or `Receiving: name`.
local function file_of(said)
    local name = string.match(said, "^%s*Sending:%s*(.-)%s*$")
    if name and name ~= "" then
        return "send", name
    end
    name = string.match(said, "^%s*Receiving:%s*(.-)%s*$")
    if name and name ~= "" then
        return "receive", name
    end
    return nil
end

-- What a modem program said, a line at a time.
--
-- These programs overwrite one line as they go: what they write is pieces
-- ended by a carriage return, not lines ended by a newline, so reading lines
-- would hold the whole of a transfer in a buffer and say it all at the end.
local function said_by(program, left)
    local came = program:read_err(nil, 200)
    if came then
        left = left .. came
    end

    local lines = {}
    while true do
        local at = string.find(left, "[\r\n]")
        if not at then
            break
        end
        local piece = string.sub(left, 1, at - 1)
        left = string.sub(left, at + 1)
        if string.match(piece, "%S") then
            lines[#lines + 1] = piece
        end
    end
    return lines, left, came ~= nil
end

-- Asks what the device has to run, and whether it has to be told at all.
--
-- The names are the ones `lrzsz` installs. The switch is for a device that was
-- already told by hand — a board whose loader is waiting for a file, a program
-- somebody started in another window — because a command typed into a device
-- that is already receiving is a command that lands in the file.
--
-- A modem taking a file off the device cannot guess which file: the program at
-- the far end is told the name, and only the person at the console knows it.
--
-- `fields` are asked after the two, and their answers come back beside
-- `remote`.
function modem.ask(how)
    local fields = {
        {
            name = "use_remote",
            kind = "switch",
            label = "Use remote command",
            value = how.use_remote ~= false,
            hint = "Tell the device what to run. Off for a device that is already waiting",
        },
        {
            name = "remote",
            kind = "text",
            label = "Remote command",
            value = how.value,
            hint = how.remote_hint or "What the device runs, as lrzsz names it",
        },
    }
    for _, field in ipairs(how.fields or {}) do
        fields[#fields + 1] = field
    end
    local answers = zyt.ui.ask{
        title = how.title,
        hint = how.hint,
        fields = fields,
    }
    if not answers then
        return nil
    end

    if not answers.use_remote or not string.match(answers.remote or "", "%S") then
        answers.remote = nil
    end
    return answers
end

-- Tells the device what to run, when there is something to tell it, and waits
-- until it has taken it.
function modem.tell(remote)
    if remote then
        zyt.progress.stage{ kind = "telling", text = remote }
        zyt.line.type(remote)
        modem.taken(modem.DELAY)
    end
end

-- Runs one modem transfer.
--
-- Waits until the device has taken the command, or until the time is up.
--
-- A console echoes what was typed into it, so the first thing back is the sign
-- that the shell is reading the line; a line that echoes nothing waits out the
-- bound instead. What is waiting is looked at and not read: a byte read here
-- would be a byte missing from the program that is about to be handed the
-- line.
local function taken(delay)
    local left = delay
    while left > 0 do
        if zyt.line.waiting() > 0 then
            zyt.time.sleep(modem.SETTLE)
            return
        end
        local slice = math.min(left, 50)
        zyt.time.sleep(slice)
        left = left - slice
    end
end

-- The same wait, for a script that speaks the protocol itself and has to know
-- when the device has taken the command it was given.
modem.taken = taken

-- `remote` is the command line the device is told to run, `delay` the longest
-- it is given to take it, `command` and `args` the program here, and `cwd` the
-- directory it runs in.
function modem.carry(how)
    if how.remote then
        zyt.progress.stage{ kind = "telling", text = how.remote }
        zyt.line.type(how.remote)
        taken(how.delay or modem.DELAY)
    end

    local program = zyt.proc.spawn{
        command = how.command,
        args = how.args,
        cwd = how.cwd,
    }
    local carrying = program:attach_to_line{ stderr = "none" }

    modem.report{ kind = "negotiating" }
    local left = ""
    local files = 0
    while true do
        local lines, rest, came = said_by(program, left)
        left = rest
        for _, said in ipairs(lines) do
            local done, whole = bytes_of(said)
            local direction, name = file_of(said)
            if done then
                modem.report{ kind = "bytes", done = done, whole = whole }
            elseif direction then
                files = files + 1
                modem.report{ kind = "file", direction = direction, name = name, index = files }
            else
                zyt.term.print(said)
            end
        end
        if not came and not program:running() then
            break
        end
    end

    local code = carrying:wait(600000)
    zyt.progress.clear()

    if code and code ~= 0 then
        error(string.format("%s ended with %d", how.command, code), 0)
    end
    zyt.notice.info(string.format("%s is done", how.command))
end

return modem
