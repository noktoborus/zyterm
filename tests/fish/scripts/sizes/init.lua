-- A script that gives the line the size of a far terminal, then waits.
return {
    send = function()
        assert(zyt.port.params() == nil, "a console has no port")
        zyt.line.resize(100, 30)
        zyt.line.resize(500, 500)
        zyt.time.sleep(600000)
    end,
}
