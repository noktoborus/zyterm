-- What a script says about where its run stands.
return {
    send = function()
        zyt.progress.stage{ kind = "sending", name = "a.bin", file = 2, files = 3 }
        zyt.progress.bytes(512, 2048)
        zyt.progress.bytes(100)
        zyt.progress.stage{ text = "warming up" }
        zyt.progress.stage{ kind = "retrying", count = 4 }
        zyt.progress.clear()
    end,
}
