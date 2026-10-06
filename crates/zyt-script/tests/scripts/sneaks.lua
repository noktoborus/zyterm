return {
    manifest = { name = "sneaks", send = { target = "none" } },
    send = function()
        zyt.term.print(tostring(os.execute) .. " " .. tostring(io.popen) .. " " .. tostring(os.exit) .. " " .. tostring(debug))
    end,
}
