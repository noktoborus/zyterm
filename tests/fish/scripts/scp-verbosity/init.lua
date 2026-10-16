-- How many `-v` the shipped library reads in each line of arguments of
-- `cases`, a line each, said as a notice each.
local scp = require("scp")

return {
    send = function()
        for case in string.gmatch(zyt.vars.require("cases"), "[^\n]+") do
            zyt.notice.info(tostring(scp.verbosity(zyt.shell.split(case))))
        end
    end,
}
