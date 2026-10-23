-- A script that sets the port the way a client of RFC 2217 asks, then ends.
return {
    send = function()
        local params = zyt.port.params()
        assert(params and params.baud == 115200, "the port says its speed")
        assert(zyt.port.set_params({ baud = 9600, parity = "even" }))
        assert(not zyt.port.set_params({ parity = "mark" }), "no mark parity")
        assert(zyt.port.set_rts(true))
        assert(zyt.port.lines(), "the lines of the modem")
    end,
}
