-- ZModem, the one most devices have, spoken here with `zyt.zmodem`.
--
-- The device runs `rz` or `sz` from `lrzsz`, or whatever it has that speaks
-- the protocol; this side speaks it itself, so nothing has to be installed
-- here and the files the device offers can be taken one by one.
--
-- What goes and where it lands are picked before the script runs, as for
-- XModem and YModem: the files to send, any number of them, since the protocol
-- carries a batch with the name and the length of each; the directory what
-- comes back lands in. Both forms come up every time, because what the device
-- is told to run is what a run is. What was answered is kept and filled in.

local modem = require("modem")
local fish = require("fish")

-- How long the device is given to answer a listing.
local LISTING_TIMEOUT = 20000

-- How long the line has to stay quiet before the device is taken to stand at
-- its prompt again.
local QUIET = 300

-- The longest the device is waited for between the two passes.
local QUIET_MOST = 5000

-- Tells the device what to run and waits until it has taken it.
local function run_there(command)
    zyt.progress.stage{ kind = "telling", text = command }
    zyt.line.type(command)
    modem.taken(modem.DELAY)
end

-- What the first of two passes says while it only learns what is offered:
-- that it is listing, and nothing of the files it passes over.
local function listing(event)
    if event.kind == "negotiating" then
        zyt.progress.stage{ kind = "listing" }
        zyt.progress.indeterminate()
    end
end

-- Waits for the device to stop talking: the program of the last pass ending,
-- and its shell coming back with a prompt.
local function settle()
    local waited = 0
    while waited < QUIET_MOST do
        if not zyt.line.read(QUIET) then
            return
        end
        waited = waited + QUIET
    end
end

-- What the far end calls a file, said with its size.
local function offered_label(name, size)
    if size then
        return string.format("%s  %s", name, zyt.codec.short_size(size))
    end
    return name
end

-- Says what a receive came to.
local function report(files, into)
    local taken = 0
    for _, file in ipairs(files) do
        if file.taken then
            taken = taken + 1
        end
    end
    zyt.notice.info(string.format("%d of %d offered taken into %s", taken, #files, into))
end

-- The files a shell on the device lists in the directory it stands in, picked
-- by the person, and the command that sends them.
local function by_listing(remote)
    zyt.progress.stage{ kind = "listing" }
    zyt.progress.indeterminate()
    local session = fish.session.new{ timeout = LISTING_TIMEOUT }
    session:hello()
    local where = session:standing()

    local options = {}
    for _, entry in ipairs(session:list(where)) do
        if entry.kind == "file" then
            options[#options + 1] = { entry.name, offered_label(entry.name, entry.size) }
        end
    end
    if #options == 0 then
        zyt.notice.info(string.format("%s holds no file", where))
        return nil
    end

    local picked = zyt.ui.ask{
        id = "listed",
        unsaved = true,
        title = "What to take off the device",
        fields = {
            { name = "names", kind = "many_of", label = where, options = options, required = true },
        },
    }
    if not picked then
        return nil
    end

    local words = { remote }
    for _, name in ipairs(picked.names) do
        local path = (where == "/") and ("/" .. name) or (where .. "/" .. name)
        words[#words + 1] = zyt.shell.quote_posix(path)
    end
    return table.concat(words, " ")
end

-- Asks which of the files the device offered are wanted. One file is wanted
-- without asking: there is nothing to choose between.
local function chosen_of(offered)
    local names, seen = {}, {}
    for _, file in ipairs(offered) do
        if not seen[file.name] then
            seen[file.name] = true
            names[#names + 1] = file
        end
    end
    if #names <= 1 then
        return names[1] and { [names[1].name] = true } or {}
    end

    local options = {}
    for _, file in ipairs(names) do
        options[#options + 1] = { file.name, offered_label(file.name, file.size) }
    end
    local picked = zyt.ui.ask{
        id = "offered",
        unsaved = true,
        title = "What the device offers",
        fields = {
            { name = "names", kind = "many_of", label = "Offered", options = options, required = true },
        },
    }
    if not picked then
        return nil
    end
    local chosen = {}
    for _, name in ipairs(picked.names) do
        chosen[name] = true
    end
    return chosen
end

-- Two runs of the same command: the first only learns what the device offers,
-- skipping every file, and the second takes what was picked.
local function two_passes(remote, into, existing)
    run_there(remote)
    local first = zyt.zmodem.receive{
        into = into,
        on = listing,
        decide = function()
            return "skip"
        end,
    }
    if #first == 0 then
        zyt.notice.info("the device offered nothing")
        return nil
    end

    local chosen = chosen_of(first)
    if not chosen then
        return nil
    end

    settle()
    run_there(remote)
    local second = zyt.zmodem.receive{
        into = into,
        on = modem.report,
        existing = existing,
        decide = function(file)
            return chosen[file.name] and "take" or "skip"
        end,
    }

    local seen = {}
    for _, file in ipairs(second) do
        seen[file.name] = true
    end
    for name in pairs(chosen) do
        if not seen[name] then
            zyt.notice.error(string.format("%s was not offered the second time", name))
        end
    end
    return second
end

-- Sends those files in one batch, whether they were picked or dropped onto
-- the window.
local function send(paths)
    local how = zyt.ui.ask{
        id = "send",
        always = true,
        title = "ZModem: onto the device",
        fields = {
            {
                name = "use_remote",
                kind = "switch",
                label = "Run the command on the device",
                value = true,
                hint = "Off for a device that is already waiting, or whose shell starts rz when the line says so",
            },
            {
                name = "remote",
                kind = "text",
                label = "Command on the device",
                value = "rz -y",
                hint = "What the device runs to receive",
            },
        },
    }
    if not how then
        return
    end

    local runs = how.use_remote and string.match(how.remote or "", "%S") ~= nil
    if runs then
        run_there(how.remote)
    end
    zyt.zmodem.send{ paths = paths, announce = not runs, on = modem.report }
    if #paths == 1 then
        zyt.notice.info(string.format("%s is on the device", paths[1]))
    else
        zyt.notice.info(string.format("%d files are on the device", #paths))
    end
end

return {

    send = function()
        local paths = zyt.ui.pick{ kind = "files", title = "ZModem: what goes onto the device" }
        if paths then
            send(paths)
        end
    end,

    drop = function()
        send(zyt.target.paths())
    end,

    receive = function()
        local picked = zyt.ui.pick{ kind = "directory", title = "ZModem: where what comes back lands" }
        if not picked then
            return
        end
        local into = picked[1]
        local how = zyt.ui.ask{
            id = "receive",
            always = true,
            title = "ZModem: off the device",
            fields = {
                {
                    name = "existing",
                    kind = "one_of",
                    label = "A file already there",
                    value = "rename",
                    options = {
                        { "rename", "Keep it, save the new one beside it" },
                        { "resume", "Take it for the start and fetch the rest" },
                        { "overwrite", "Write over it" },
                        { "skip", "Keep it, skip the new one" },
                    },
                    hint = "Resuming trusts the name and the size: nothing compares the bytes",
                },
                {
                    name = "listing",
                    kind = "one_of",
                    label = "Files to choose from",
                    value = "shell",
                    options = {
                        { "shell", "The directory the device's shell stands in" },
                        { "offered", "Whatever the command offers" },
                    },
                },
                {
                    name = "two_pass",
                    kind = "switch",
                    label = "Two passes to collect the files",
                    value = true,
                    hint = "For what the command offers: run it once to learn the files, pick, run it again",
                },
                {
                    name = "use_remote",
                    kind = "switch",
                    label = "Run the command on the device",
                    value = true,
                    hint = "Off for a device that is already sending",
                },
                {
                    name = "remote",
                    kind = "text",
                    label = "Command on the device",
                    value = "sz -b",
                    hint = "For a listing, the picked files are added to it; otherwise it has to name them",
                },
            },
        }
        if not how then
            return
        end

        local remote = how.remote or ""
        local existing = how.existing or "rename"
        local runs = how.use_remote and string.match(remote, "%S") ~= nil
        local files

        if how.listing == "shell" then
            if not runs then
                zyt.notice.error("a listing needs the command to be run on the device")
                return
            end
            local command = by_listing(remote)
            if not command then
                return
            end
            run_there(command)
            files = zyt.zmodem.receive{ into = into, existing = existing, on = modem.report }
        elseif how.two_pass and runs then
            files = two_passes(remote, into, existing)
            if not files then
                return
            end
        else
            if runs then
                run_there(remote)
            end
            files = zyt.zmodem.receive{ into = into, existing = existing, on = modem.report }
        end

        report(files, into)
    end,
}
