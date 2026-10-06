-- One command of the protocol, driven on its own.
--
-- The tests of the wire drive the conversation a command at a time against
-- replies written down beforehand, which is how the protocol was tested while
-- it was Rust. The case to run and what to run it on come in as values of the
-- source.

local fish = require("fish")

local function session()
    return fish.session.new{
        mode = zyt.vars.get("mode") or "base64",
        chunk = math.tointeger(tonumber(zyt.vars.get("chunk") or "2048")),
        size_check = zyt.vars.get("size_check") ~= "off",
        timeout = 2000,
    }
end

local cases = {}

function cases.pwd(line)
    zyt.term.print(line:pwd())
end

function cases.probe(line)
    line:hello()
    zyt.term.print("the device has what it needs")
end

function cases.offers(line)
    zyt.term.print(tostring(line:offers_digest(zyt.vars.require("program"))))
end

function cases.canonical(line)
    zyt.term.print(line:canonical(zyt.vars.require("path")))
end

function cases.kind(line)
    zyt.term.print(line:kind(zyt.vars.require("path")))
end

function cases.size(line)
    zyt.term.print(tostring(line:size(zyt.vars.require("path"))))
end

function cases.digest(line)
    zyt.term.print(line:digest(zyt.vars.require("program"), zyt.vars.require("path")))
end

function cases.list(line)
    for _, entry in ipairs(line:list(zyt.vars.require("path"))) do
        zyt.term.print(string.format("%s %d %s", entry.kind, entry.size, entry.name))
    end
end

function cases.make_directory(line)
    line:make_directory(zyt.vars.require("path"))
    zyt.term.print("made")
end

function cases.create(line)
    line:create(zyt.vars.require("path"))
    zyt.term.print("created")
end

function cases.retrieve(line)
    local carried = line:retrieve(zyt.vars.require("path"), zyt.vars.require("into"))
    zyt.term.print(tostring(carried))
end

function cases.store(line)
    local carried = line:store(zyt.vars.require("from"), zyt.vars.require("path"))
    zyt.term.print(tostring(carried))
end

function cases.walk(line)
    for _, item in ipairs(fish.walk.remote(line, { zyt.vars.require("path") })) do
        zyt.term.print(string.format("%s %d %s", item.relative, item.size, item.path))
    end
end

return {

    send = function()
        local case = zyt.vars.require("case")
        local body = cases[case]
        if not body then
            error(string.format("there is no case called %s", case), 0)
        end
        body(session())
    end,
}
