-- The line, offered on a port of this machine to a telnet client.
--
-- One client at a time: what it types goes to the device, what the device
-- says goes to it and then into the terminal, which is the only place a
-- script holding the line has to show it. A second client is told the port is
-- taken and let go. A client leaving leaves the port listening for the next
-- one; the run ends when it is stopped or the line goes, and whatever it opened
-- is closed with it.
--
-- This end echoes, suppresses the go-ahead and sends in binary. It takes the
-- size of the window of a client (NAWS, RFC 1073) for a console, and on a
-- serial port it serves RFC 2217 (option 44): the client sets the speed, the
-- frame, the modem lines and the break, and is told the modem lines. Every
-- other option is refused, and the commands of the protocol are taken out of
-- what goes to the device. A client that connects late gets the state of the
-- terminal first: the modes, the character sets, the cursor and the title.

-- The bytes of the protocol.
local IAC, DONT, DO, WONT, WILL, SB, SE, IP = 255, 254, 253, 252, 251, 250, 240, 244
local BINARY, ECHO, SGA, NAWS, COM_PORT = 0, 1, 3, 31, 44

-- How often the speed is said, each way, and the modem lines looked at.
local SPEED_EVERY = 1000

-- What a client is told when the port is already taken.
local BUSY = "Another client holds this line.\r\n"

-- What this end says when a client connects.
local OFFER = string.char(
    IAC, WILL, ECHO, IAC, WILL, SGA, IAC, DO, SGA,
    IAC, WILL, BINARY, IAC, DO, BINARY, IAC, DO, NAWS
)

-- What this end does, and what it asked the client to do.
local WILLING = { [ECHO] = true, [SGA] = true, [BINARY] = true }
local ASKED = { [SGA] = true, [BINARY] = true, [NAWS] = true }

-- The features of the terminal that reach past the window of zyterm: a client
-- drives them, and they are not for this desktop.
local FEATURES = {
    notifications = false, directory = false, clipboard = false,
    progress = false, bell = false, marks = false,
}

-- RFC 2217: the codes of a parity and of a flow control, and the modem lines
-- by their bit in a notice of the modem state.
local PARITY = { none = 1, odd = 2, even = 3 }
local FLOW = { none = 1, software = 2, hardware = 3 }
local MODEM = { cts = 16, dsr = 32, ri = 64, cd = 128 }

-- An unsigned number of `size` bytes, most significant first.
local function number(bytes, size)
    local value = 0
    for i = 1, size do
        value = value * 256 + (bytes:byte(i) or 0)
    end
    return value
end

local function bytes_of(value, size)
    local out = {}
    for i = size, 1, -1 do
        out[i] = string.char(value % 256)
        value = value // 256
    end
    return table.concat(out)
end

-- A byte of 255 doubled, or it would be read as a command.
local function escaped(data)
    if not data:find("\255", 1, true) then
        return data
    end
    return (data:gsub("\255", "\255\255"))
end

-- The answer to a command of RFC 2217: the code plus 100, and the value as
-- the port has it now.
local function sub(code, value)
    return string.char(IAC, SB, COM_PORT, code + 100) .. escaped(value) .. string.char(IAC, SE)
end

local function key_of(map, wanted)
    for name, code in pairs(map) do
        if code == wanted then
            return name
        end
    end
end

-- Sets one field of the line and gives the line that came of it: what the
-- driver has after the set, or the line as it stands when the port refused or
-- is not open. A client that is answered unlike its request takes it as
-- refused, which is what RFC 2217 has for a refusal.
local function set(port, field, value)
    return port.set_params{ [field] = value } or port.params()
end

-- Does what a client of RFC 2217 asks, and says what came of it.
local function com_port(code, value)
    local port, asked = zyt.port, value:byte(1) or 0
    if code == 0 then
        return #value == 0 and sub(0, "zyterm") or ""
    elseif code == 1 then
        local baud = number(value, 4)
        local line = baud > 0 and set(port, "baud", baud) or port.params()
        return sub(1, bytes_of(line.baud, 4))
    elseif code == 2 then
        local line = asked > 0 and set(port, "data_bits", asked) or port.params()
        return sub(2, string.char(line.data_bits))
    elseif code == 3 then
        local parity = key_of(PARITY, asked)
        local line = parity and set(port, "parity", parity) or port.params()
        return sub(3, string.char(PARITY[line.parity]))
    elseif code == 4 then
        local stop = (asked == 1 or asked == 2) and asked
        local line = stop and set(port, "stop_bits", stop) or port.params()
        return sub(4, string.char(line.stop_bits))
    elseif code == 5 then
        if asked == 0 then
            asked = FLOW[port.params().flow] or FLOW.hardware
        elseif asked <= 3 then
            asked = FLOW[set(port, "flow", key_of(FLOW, asked)).flow] or FLOW.hardware
        elseif asked == 5 or asked == 6 then
            port.set_break(asked == 5)
        elseif asked == 7 or asked == 8 or asked == 9 then
            if asked ~= 7 then
                port.set_dtr(asked == 8)
            end
            asked = port.lines().dtr and 8 or 9
        elseif asked == 10 or asked == 11 or asked == 12 then
            if asked ~= 10 then
                port.set_rts(asked == 11)
            end
            asked = port.lines().rts and 11 or 12
        elseif asked == 4 then
            asked = 6
        elseif asked == 13 then
            asked = 14
        end
        return sub(5, string.char(asked))
    elseif code == 8 or code == 9 then
        port.hold(code == 8)
        return ""
    elseif code == 12 then
        if asked == 2 or asked == 3 then
            port.purge()
        end
        return sub(12, value)
    elseif code == 10 or code == 11 then
        return sub(code, value)
    end
    return ""
end

-- Reads what a client sends: the bytes for the device, and the answers the
-- protocol owes it. The state lives across reads, because a command may be
-- cut between two of them.
-- `client.com_port` is set once the client agreed to RFC 2217.
local function reader(serial, client)
    local state, verb, payload = "data", nil, {}
    local binary = false

    local function option(answers, byte)
        if verb == WILL then
            if byte == BINARY then
                binary = true
            end
            if byte == COM_PORT and serial then
                client.com_port = true
                answers[#answers + 1] = string.char(IAC, DO, COM_PORT)
            elseif not ASKED[byte] then
                answers[#answers + 1] = string.char(IAC, DONT, byte)
            end
        elseif verb == WONT then
            if byte == BINARY then
                binary = false
            end
        elseif verb == DO and not WILLING[byte] then
            answers[#answers + 1] = string.char(IAC, WONT, byte)
        end
    end

    local function negotiated(answers)
        local data = table.concat(payload)
        payload = {}
        local of = data:byte(1)
        if of == NAWS and #data >= 5 then
            local columns, rows = number(data:sub(2), 2), number(data:sub(4), 2)
            if columns > 0 and rows > 0 then
                zyt.line.resize(columns, rows)
            end
        elseif of == COM_PORT and serial and #data >= 2 then
            answers[#answers + 1] = com_port(data:byte(2), data:sub(3))
        end
    end

    return function(data)
        local out, answers = {}, {}
        for i = 1, #data do
            local byte = data:byte(i)
            if state == "data" then
                if byte == IAC then
                    state = "iac"
                elseif byte == 13 and not binary then
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
                state = "data"
                if byte == IAC then
                    out[#out + 1] = string.char(IAC)
                elseif byte >= WILL then
                    verb, state = byte, "option"
                elseif byte == SB then
                    state = "sub"
                elseif byte == IP then
                    out[#out + 1] = "\3"
                end
            elseif state == "option" then
                option(answers, byte)
                state = "data"
            elseif state == "sub" then
                if byte == IAC then
                    state = "sub_iac"
                else
                    payload[#payload + 1] = string.char(byte)
                end
            elseif state == "sub_iac" then
                if byte == SE then
                    negotiated(answers)
                    state = "data"
                else
                    payload[#payload + 1] = string.char(byte)
                    state = "sub"
                end
            end
        end
        return table.concat(out), table.concat(answers)
    end
end

-- Asks where to listen, and whether a client that connects late gets the
-- state of the terminal.
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
            {
                name = "send_state",
                kind = "switch",
                label = "Send the terminal state",
                value = true,
                hint = "A client that connects later gets the modes and the title a program set",
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
    return how.host, port, how.send_state ~= false
end

-- The modem lines as a notice of RFC 2217 says them.
local function modem_state()
    local lines, state = zyt.port.lines(), 0
    for name, bit in pairs(MODEM) do
        if lines[name] then
            state = state | bit
        end
    end
    return state
end

return {

    receive = function()
        local host, port, send_state = ask()
        if not host then
            return
        end
        zyt.term.features(FEATURES)

        local serial = zyt.port.params() ~= nil
        local listener = zyt.net.listen{ host = host, port = port }
        local address = listener:address()
        local client, read, modem, talk = nil, nil, nil, {}
        local received, sent, said_at = 0, 0, zyt.time.now_ms()

        local function waiting_for_one()
            client = nil
            zyt.progress.stage{ text = string.format("Waiting for a client on %s", address) }
        end
        waiting_for_one()
        zyt.notice.info(string.format("telnet %s", (address:gsub(":(%d+)$", " %1"))))

        -- One wait for the device, the client and somebody connecting; it
        -- wakes as soon as one of them has something, and in time to say the
        -- speed.
        while true do
            local now = zyt.time.now_ms()
            local sockets = { listener }
            if client then
                sockets[2] = client
            end
            local ready = zyt.wait{
                line = true,
                sockets = sockets,
                timeout = math.max(0, SPEED_EVERY - (now - said_at)),
            }

            if ready.line then
                local data = zyt.line.read(0)
                if data then
                    if client and not client:write(escaped(data)) then
                        waiting_for_one()
                    end
                    zyt.term.write(data)
                    received = received + #data
                end
            end

            now = zyt.time.now_ms()
            if now - said_at >= SPEED_EVERY then
                local span = now - said_at
                zyt.progress.speed{
                    received = received * 1000 // span,
                    sent = sent * 1000 // span,
                }
                received, sent, said_at = 0, 0, now
                local state = talk.com_port and modem_state()
                if client and state and state ~= modem then
                    local changed = ((modem or state) ~ state) >> 4
                    client:write(sub(7, string.char(state | changed)))
                    modem = state
                end
            end

            if client and ready[client] then
                local data, why = client:read(0)
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
                    waiting_for_one()
                end
            end

            if ready[listener] then
                local another = listener:accept(0)
                if another and client then
                    another:write(BUSY)
                    another:close()
                elseif another then
                    talk = {}
                    client, read, modem = another, reader(serial, talk), nil
                    client:write(OFFER)
                    local state = send_state and zyt.term.state()
                    if state then
                        client:write(escaped(state))
                    end
                    zyt.progress.stage{ text = string.format("%s on %s", client:peer(), address) }
                end
            end
        end
    end,
}
