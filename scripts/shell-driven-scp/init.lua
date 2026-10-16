-- Files over the network, into and out of the directory the device stands in.
--
-- It carries no byte over the line: the line is held only long enough to ask
-- the device where it stands and what it holds, and `scp` does the rest over
-- whatever network the two share. That is what makes it worth having -- a
-- serial line is slow and a network is not, and the two things the network
-- does not know are which directory the person at the console is working in
-- and what is in it.

local fish = require("fish")
local scp = require("scp")

-- What `scp` is given before the paths, when nobody said otherwise. `-vv` is
-- what lets the run say where it stands (`lib/scp.lua`); with less, it runs
-- with no share at all.
local DEFAULT_ARGS = "-O -vv"

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

-- Where the device stands, asked over the line and said in the terminal.
local function standing(session)
    local where = session:standing()
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

-- One path of the device as `scp` is given it.
--
-- The far side of an `scp` is a shell: the path after the colon is read by it,
-- so a name with a space in it is two paths there unless it is quoted. The
-- quotes are not read on this side -- nothing here runs through a shell -- so
-- they cross as they stand and are taken off by the one reader that should.
local function remote(user, host, path)
    return string.format("%s@%s:%s", user, host, zyt.shell.quote_posix(path))
end

-- What `scp` is given so that it takes the name the device sends.
--
-- Taking a file, `scp` holds the asked path against the name the far side
-- announces and ends with `filename does not match request` when the two
-- differ. The quotes the far shell needs are in the asked path and not in the
-- name, so every path of this script differs that way; `-T` is what says the
-- device is the one trusted to name what it sends.
local UNCHECKED = "-T"

-- Runs `scp`, says where it stands from what it says at `-vv`, and prints
-- what else it says. `files` are the files of the run in order, each
-- `{ name = , size = }`.
local function carry(words, direction, files)
    local reader = scp.reader{
        direction = direction,
        files = files,
        verbose = scp.verbosity(words),
    }
    local program = zyt.proc.spawn{ command = "scp", args = words }
    while true do
        local said = program:read_err_line(200)
        if said then
            if string.match(said, "%S") and not reader:take(said) then
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
    reader:finish()
    zyt.notice.info("scp is done")
end

-- The files of a directory of the device, by name, and the size of each.
local function files_of(session, where)
    local picked, sizes = {}, {}
    for _, entry in ipairs(session:list(where)) do
        if entry.kind == "file" then
            picked[#picked + 1] = {
                entry.name,
                string.format("%s  %s", entry.name, zyt.codec.short_size(entry.size)),
            }
            sizes[entry.name] = entry.size
        end
    end
    return picked, sizes
end

-- The last part of a path of this side.
local function base_name(path)
    return string.match(path, "([^/\\]*)$") or path
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
        local files = {}
        for _, path in ipairs(zyt.target.paths()) do
            words[#words + 1] = path
            local stat = zyt.fs.stat(path) or {}
            files[#files + 1] = { name = base_name(path), size = stat.size }
        end
        words[#words + 1] = remote(user, host, where)
        carry(words, "send", files)
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
        local offered, sizes = files_of(session, where)
        if #offered == 0 then
            zyt.notice.info(string.format("%s holds no file", where))
            return
        end

        -- What the device holds now is no answer to the next run: the listing
        -- is this moment's, so the form is one nothing is kept for. The
        -- directory the files land in is one row of it and not a question put
        -- before the run, because it is answered against the listing: what is
        -- being taken is what says where it should go. The list is headed by
        -- the directory the device stands in, which is what the names are
        -- names in.
        local which = zyt.ui.ask{
            id = "files",
            unsaved = true,
            title = "What to take off the device",
            hint = string.format("On %s", host),
            fields = {
                {
                    name = "files",
                    kind = "many_of",
                    label = where,
                    options = offered,
                    required = true,
                },
                {
                    name = "into",
                    kind = "path",
                    directory = true,
                    label = "Download to directory",
                    hint = "Where on this side the files land; the dialog opens where it was left",
                    required = true,
                },
            },
        }
        if not which then
            return
        end

        local words = zyt.shell.split(how.args)
        words[#words + 1] = UNCHECKED
        local files = {}
        for _, name in ipairs(which.files) do
            words[#words + 1] = remote(user, host, below(where, name))
            files[#files + 1] = { name = name, size = sizes[name] }
        end
        words[#words + 1] = which.into
        carry(words, "receive", files)
    end,
}
