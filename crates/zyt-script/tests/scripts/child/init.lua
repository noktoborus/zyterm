-- A script that starts a long program and waits for it.
return {
    send = function()
        local program = zyt.proc.spawn{ command = "sh", args = { "-c", "sleep 600 & echo $!; wait" } }
        local said = program:read_line(5000)
        zyt.term.print("pid " .. (said or "none"))
        program:wait(600000)
    end,
}
