-- A script whose end is a key it sends the device once the line is free.
return {
    send = function()
        zyt.term.print("keyed")
        zyt.line.wait_drained()
        zyt.line.write_key("ctrl+c")
    end,
}
