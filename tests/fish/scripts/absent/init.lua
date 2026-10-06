-- A script that asks for a program the machine has not got.
local modem = require("modem")

return {
    send = function()
        modem.carry{
            remote = "true",
            delay = 10,
            command = "nothing-of-the-sort",
            args = { zyt.target.path() },
        }
    end,
}
