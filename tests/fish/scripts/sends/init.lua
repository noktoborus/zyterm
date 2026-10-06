-- A script that hands a program the line and lets it carry the file.
return {
    send = function()
        local program = zyt.proc.spawn{ command = "cat", args = { zyt.target.path() } }
        local carrying = program:attach_to_line{ stderr = "notice" }
        carrying:wait(600000)
        zyt.time.sleep(600000)
    end,
}
