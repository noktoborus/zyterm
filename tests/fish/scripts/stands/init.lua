-- A script that says it is sending the second of five files, half of it gone.
return {
    send = function()
        zyt.progress.stage{ kind = "sending", name = "foo.bin", file = 2, files = 5 }
        zyt.progress.bytes(2097152, 4194304)
        zyt.time.sleep(600000)
    end,
}
