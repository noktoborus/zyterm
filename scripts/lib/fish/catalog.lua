-- The shell of every command, and the two modes a body travels in.
--
-- Every command is a file under `sh/`, written the way shell reads best and
-- folded into the one line the wire takes. The four a mode owns stand in
-- `sh/base64/` and `sh/raw/`; the rest are shared. Nothing here is a Lua
-- string: a script is worth reading as a script.

local catalog = {}

-- How many bytes of a file make one line of base64, and what that line is.
catalog.LINE_BYTES = 57
catalog.LINE_CHARS = 76

-- The word that closes the here-document a chunk of base64 travels in.
catalog.HEREDOC = "SHXFER_EOF"

-- How many bytes of the line one chunk takes on a console.
--
-- A console carries the bytes one after another, so a chunk costs what it
-- carries and the round trip that ends it is a rounding error. Small chunks
-- are what say where a transfer stopped.
catalog.DEFAULT_CHUNK = 2048

-- How many bytes of the line one chunk takes over a pipe.
--
-- A pipe is usually a shell somewhere else, and then a chunk costs a round
-- trip over whatever stands between. What travels between two round trips is
-- what the transfer is worth, so a chunk is large where the waiting is.
catalog.DEFAULT_PIPE_CHUNK = 65536

local SH = "lib/fish/sh"

-- The name a template file goes by in the catalogue.
local function named(file)
    local stem = string.gsub(file, "%.sh$", "")
    return (string.gsub(stem, "-", "_"))
end

-- Every template of that directory, folded into one line each.
local function take(into, directory)
    for _, file in ipairs(zyt.script.list_files(directory)) do
        if string.match(file, "%.sh$") then
            into[named(file)] = zyt.script.fold(zyt.script.read_file(directory .. "/" .. file))
        end
    end
end

-- The whole set of commands one mode sends.
function catalog.new(mode)
    if mode ~= "base64" and mode ~= "raw" then
        error(string.format("there is no mode called %s", tostring(mode)), 0)
    end

    local commands = {}
    take(commands, SH)
    take(commands, SH .. "/" .. mode)

    return {
        mode = mode,
        commands = commands,
        -- How the body of a chunk reaches the device: a here-document the
        -- shell reads itself, or bytes counted out by `head -c`.
        body = (mode == "raw") and "counted" or "document",
    }
end

-- The template of that command, or a complaint naming it.
function catalog.command(set, name)
    local template = set.commands[name]
    if not template then
        error(string.format("the mode %s has no command %s", set.mode, name), 0)
    end
    return template
end

-- The probe of one sum, when this protocol has one.
function catalog.probe(set, program)
    return set.commands["probe_" .. program]
end

-- How many bytes of a file one chunk of that many bytes on the wire carries.
--
-- Raw carries the chunk itself; base64 carries as many whole lines as fit, at
-- least one, because half a line decodes to nothing the device can place.
function catalog.slice(set, on_wire)
    if set.mode == "raw" then
        return math.max(on_wire, 1)
    end
    return zyt.codec.lines_of(on_wire)
end

-- The body of a chunk as the scripts of this mode will read it.
function catalog.body_of(set, bytes)
    if set.mode == "raw" then
        return bytes
    end
    return zyt.codec.b64_encode(bytes)
end

return catalog
