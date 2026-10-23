-- A script that starts a program which ignores the ask to stop, and waits.
return {
    send = function()
        zyt.proc.spawn{ command = "sh", args = { "-c", "trap '' TERM HUP; exec sleep 600" } }
        zyt.time.sleep(600000)
    end,
}
