-- A script that leaves the line alone, says how far it is and waits.
return {
    send = function()
        zyt.progress.share(40)
        zyt.time.sleep(600000)
    end,
}
