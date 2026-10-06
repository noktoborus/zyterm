-- A script talking to a program through its pipes.
return {
    send = function()
        local program = zyt.proc.spawn{ command = "sh", args = { "-c", "read one; echo \"got $one\"; echo bad 1>&2" } }
        program:write("hello\n")
        program:close_stdin()
        local said = program:read_line(5000)
        local complaint = program:read_err_line(5000)
        local code = program:wait(5000)
        zyt.term.print(said .. " code " .. tostring(code) .. " said " .. complaint)
    end,
}
