-- A script waiting on a device that says nothing.
return {
    send = function()
        zyt.line.read(600000)
        zyt.term.print("never")
    end,
}
