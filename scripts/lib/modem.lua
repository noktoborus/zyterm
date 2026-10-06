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

-- What `-vv` writes about where a transfer stands, when it writes anything.
--
-- `lrzsz` says `Bytes Sent: 4096/ 8192` and `Bytes Received` the other way
-- round. Without the total it says only how much has gone, and then there is
-- no share to show.
local function share_of(said)
    local done, whole = string.match(said, "Bytes%s+%a+:%s*(%d+)/%s*(%d+)")
    if not done then
        return nil
    end
    done, whole = tonumber(done), tonumber(whole)
    if not whole or whole == 0 then
        return nil
    end
    return math.floor((done * 100) / whole)
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
function modem.ask(how)
    local answers = zyt.ui.ask{
        title = how.title,
        hint = how.hint,
        fields = {
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
        },
    }
    if not answers then
        return nil
    end

    local remote = answers.remote
    if not answers.use_remote or not string.match(remote or "", "%S") then
        remote = nil
    end
    return { remote = remote }
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

-- `remote` is the command line the device is told to run, `delay` the longest
-- it is given to take it, `command` and `args` the program here, and `cwd` the
-- directory it runs in.
function modem.carry(how)
    if how.remote then
        zyt.line.type(how.remote)
        taken(how.delay or modem.DELAY)
    end

    local program = zyt.proc.spawn{
        command = how.command,
        args = how.args,
        cwd = how.cwd,
    }
    local carrying = program:attach_to_line{ stderr = "none" }

    zyt.progress.indeterminate()
    local left = ""
    while true do
        local lines, rest, came = said_by(program, left)
        left = rest
        for _, said in ipairs(lines) do
            local share = share_of(said)
            if share then
                zyt.progress.share(share)
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
