-- The line, offered on a port of this machine to a telnet client.
--
-- One client at a time: what it types goes to the device, what the device
-- says goes to it and into the terminal, which is the only place a script
-- holding the line has to show it. A second client is told the port is taken
-- and let go. A client leaving leaves the port listening for the next one; the
-- run ends when it is stopped or the line goes, and whatever it opened is
-- closed with it.
--
-- Telnet is spoken as little as a client needs: this end echoes and suppresses
-- the go-ahead, so a client sends every key as it is typed and shows nothing
-- of its own; every other option is refused, and the commands of the protocol
-- are taken out of what goes to the device.

-- The bytes of the protocol.
local IAC, DONT, DO, WONT, WILL, SB, SE, IP = 255, 254, 253, 252, 251, 250, 240, 244
local ECHO, SGA = 1, 3

-- How long one turn of the loop waits for the client, and for somebody to
-- connect: what a key typed there waits at most before it goes to the line.
local TURN = 10

-- How often the speed is said, each way: what the device said and what the
-- client sent it over the last second, as it was, with nothing worked out
-- from it. No share is said, so the
-- plate has no bar: a line handed to a client is not on its way to an end.
local SPEED_EVERY = 1000

-- What a client is told when the port is already taken.
local BUSY = "Another client holds this line.\r\n"

-- What this end says when a client connects: it echoes, and nobody waits for
-- a go-ahead.
local OFFER = string.char(IAC, WILL, ECHO, IAC, WILL, SGA, IAC, DO, SGA)

-- The options this end agreed to.
local OFFERED = { [ECHO] = true, [SGA] = true }

-- Reads what a client sends: the bytes for the device, and the answers the
-- protocol owes it. The state lives across reads, because a command may be
-- cut between two of them.
local function reader()
    local state = "data"
    local verb = nil

    return function(data)
        local out, answers = {}, {}
        for i = 1, #data do
            local byte = data:byte(i)
            if state == "data" then
                if byte == IAC then
                    state = "iac"
                elseif byte == 13 then
                    out[#out + 1] = "\r"
                    state = "cr"
                else
                    out[#out + 1] = string.char(byte)
                end
            elseif state == "cr" then
                -- A return is sent as CR LF or CR NUL; the device is given the
                -- CR a keyboard sends.
                state = "data"
                if byte == IAC then
                    state = "iac"
                elseif byte ~= 10 and byte ~= 0 then
                    out[#out + 1] = string.char(byte)
                    if byte == 13 then
                        state = "cr"
                    end
                end
            elseif state == "iac" then
                if byte == IAC then
                    out[#out + 1] = string.char(IAC)
                    state = "data"
                elseif byte == WILL or byte == WONT or byte == DO or byte == DONT then
                    verb = byte
                    state = "option"
                elseif byte == SB then
                    state = "sub"
                else
                    if byte == IP then
                        out[#out + 1] = "\3"
                    end
                    state = "data"
                end
            elseif state == "option" then
                if verb == WILL then
                    answers[#answers + 1] = string.char(IAC, DONT, byte)
                elseif verb == DO and not OFFERED[byte] then
                    answers[#answers + 1] = string.char(IAC, WONT, byte)
                end
                state = "data"
            elseif state == "sub" then
                if byte == IAC then
                    state = "sub_iac"
                end
            elseif state == "sub_iac" then
                state = (byte == SE) and "data" or "sub"
            end
        end
        return table.concat(out), table.concat(answers)
    end
end

-- What the device said, as a client reads it: a byte of 255 is doubled, or it
-- would be read as a command.
local function escaped(data)
    return (data:gsub("\255", "\255\255"))
end

-- Asks where to listen.
local function ask()
    local how = zyt.ui.ask{
        id = "listen",
        always = true,
        title = "Telnet: the line on a port",
        fields = {
            {
                name = "host",
                kind = "text",
                label = "Address to listen on",
                value = "127.0.0.1",
                hint = "127.0.0.1 is this machine alone; 0.0.0.0 is every network it is on",
                required = true,
            },
            {
                name = "port",
                kind = "text",
                label = "Port",
                value = "2323",
                hint = "1 to 65535; below 1024 a port needs the rights of the system",
                required = true,
            },
        },
    }
    if not how then
        return nil
    end

    local port = tonumber(how.port)
    if not port or port ~= math.floor(port) or port < 1 or port > 65535 then
        error(string.format("%s is not a port", how.port), 0)
    end
    return how.host, port
end

return {

    receive = function()
        local host, port = ask()
        if not host then
            return
        end

        local listener = zyt.net.listen{ host = host, port = port }
        local address = listener:address()
        local client, read = nil, nil
        local received, sent, said_at = 0, 0, zyt.time.now_ms()

        local function waiting_for_one()
            zyt.progress.stage{ text = string.format("Waiting for a client on %s", address) }
        end
        waiting_for_one()
        zyt.notice.info(string.format("telnet %s", (address:gsub(":(%d+)$", " %1"))))

        while true do
            if zyt.line.waiting() > 0 then
                local data = zyt.line.read(0)
                if data then
                    zyt.term.write(data)
                    if client and not client:write(escaped(data)) then
                        client = nil
                        waiting_for_one()
                    end
                    received = received + #data
                end
            end

            local now = zyt.time.now_ms()
            if now - said_at >= SPEED_EVERY then
                local span = now - said_at
                zyt.progress.speed{
                    received = received * 1000 // span,
                    sent = sent * 1000 // span,
                }
                received, sent, said_at = 0, 0, now
            end

            if client then
                local data, why = client:read(TURN)
                if data then
                    local bytes, answers = read(data)
                    if #answers > 0 then
                        client:write(answers)
                    end
                    if #bytes > 0 then
                        zyt.line.write(bytes)
                        sent = sent + #bytes
                    end
                elseif why then
                    zyt.notice.info(string.format("%s left", client:peer()))
                    client = nil
                    waiting_for_one()
                end

                local another = listener:accept(0)
                if another then
                    another:write(BUSY)
                    another:close()
                end
            else
                client = listener:accept(TURN)
                if client then
                    read = reader()
                    client:write(OFFER)
                    zyt.progress.stage{ text = string.format("%s on %s", client:peer(), address) }
                end
            end
        end
    end,
}
