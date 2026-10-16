-- What `scp` wrote to stderr in a transfer, read back line by line the way
-- the shipped script reads it while `scp` runs.
--
-- `dump` is the file of the lines, `args` the arguments `scp` was given,
-- `direction` the way the files went, and `files` the files of the run, each
-- `name=size` and the size left out where the script would not know it.
local scp = require("scp")

return {
    send = function()
        local files = {}
        for item in string.gmatch(zyt.vars.require("files"), "[^,]+") do
            local name, size = string.match(item, "^([^=]+)=?(%d*)$")
            files[#files + 1] = { name = name, size = tonumber(size) }
        end
        local reader = scp.reader{
            direction = zyt.vars.require("direction"),
            files = files,
            verbose = scp.verbosity(zyt.shell.split(zyt.vars.require("args"))),
        }

        local dump = zyt.fs.open(zyt.vars.require("dump"))
        local text = dump:read() or ""
        dump:close()
        for line in string.gmatch(text, "([^\n]*)\n") do
            if string.match(line, "%S") and not reader:take(line) then
                zyt.term.print(line)
            end
        end
        reader:finish()
    end,
}
