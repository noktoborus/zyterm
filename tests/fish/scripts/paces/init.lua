-- A script that says its bytes twice, half a second apart, and waits.
return {
    send = function()
        zyt.progress.bytes(0, 8192)
        zyt.time.sleep(500)
        zyt.progress.bytes(4096, 8192)
        zyt.time.sleep(600000)
    end,
}
