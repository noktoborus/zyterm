-- Files over the network, into and out of the directory the device stands in.
--
-- It carries no byte over the line: the line is held only long enough to ask
-- the device where it stands and what it holds, and `scp` does the rest over
-- whatever network the two share. That is what makes it worth having -- a
-- serial line is slow and a network is not, and the two things the network
-- does not know are which directory the person at the console is working in
-- and what is in it.

local fish = require("fish")

-- What `scp` is given before the paths, when nobody said otherwise.
local DEFAULT_ARGS = "-O -v"

-- How long the device may say nothing while it is asked about itself.
local TIMEOUT = 10000

-- The arguments of `scp`, asked once and kept with the connection.
local function options()
    return zyt.ui.ask{
        id = "options",
        title = "Shell-driven SCP",
        hint = "The line says where the device stands; scp carries the files",
        fields = {
            {
                name = "args",
                kind = "text",
                label = "Arguments for scp",
                hint = "Everything before the paths, read the way a shell reads a command line",
                value = DEFAULT_ARGS,
            },
        },
    }
end

-- Where the device stands, asked over the line.
local function standing(session)
    local where = session:pwd()
    if where == "" then
        error("the device would not say where it stands", 0)
    end
    zyt.notice.info(string.format("the device stands in %s", where))
    return where
end

-- One path of the device, below the directory it stands in.
local function below(where, name)
    if string.sub(where, -1) == "/" then
        return where .. name
    end
    return where .. "/" .. name
end

-- Runs `scp` and prints what it says while it runs.
local function carry(words)
    local program = zyt.proc.spawn{ command = "scp", args = words }
    while true do
        local said = program:read_err_line(200)
        if said then
            if string.match(said, "%S") then
                zyt.term.print(said)
            end
        elseif not program:running() then
            break
        end
    end

    local code = program:wait(600000)
    if code and code ~= 0 then
        error(string.format("scp ended with %d", code), 0)
    end
    zyt.notice.info("scp is done")
end

-- The files of a directory of the device, by name.
local function files_of(session, where)
    local picked = {}
    for _, entry in ipairs(session:list(where)) do
        if entry.kind == "file" then
            picked[#picked + 1] = {
                entry.name,
                string.format("%s  %s", entry.name, zyt.codec.short_size(entry.size)),
            }
        end
    end
    return picked
end

return {

    send = function()
        local user = zyt.vars.require("remote_user")
        local host = zyt.vars.require("remote_host")

        local how = options()
        if not how then
            return
        end

        local session = fish.session.new{ timeout = TIMEOUT }
        local where = standing(session)

        local words = zyt.shell.split(how.args)
        for _, path in ipairs(zyt.target.paths()) do
            words[#words + 1] = path
        end
        words[#words + 1] = string.format("%s@%s:%s", user, host, where)
        carry(words)
    end,

    receive = function()
        local user = zyt.vars.require("remote_user")
        local host = zyt.vars.require("remote_host")

        local how = options()
        if not how then
            return
        end

        local session = fish.session.new{ timeout = TIMEOUT }
        local where = standing(session)
        local offered = files_of(session, where)
        if #offered == 0 then
            zyt.notice.info(string.format("%s holds no file", where))
            return
        end

        -- What the device holds now is no answer to the next run: the listing
        -- is this moment's, so the form is one nothing is kept for.
        local which = zyt.ui.ask{
            id = "files",
            unsaved = true,
            title = "What to take off the device",
            hint = string.format("In %s on %s", where, host),
            fields = {
                {
                    name = "files",
                    kind = "many_of",
                    label = "Files",
                    options = offered,
                    required = true,
                },
            },
        }
        if not which then
            return
        end

        local words = zyt.shell.split(how.args)
        for _, name in ipairs(which.files) do
            words[#words + 1] = string.format("%s@%s:%s", user, host, below(where, name))
        end
        words[#words + 1] = zyt.target.path()
        carry(words)
    end,
}
