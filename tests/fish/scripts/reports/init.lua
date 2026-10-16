-- A script that says how far it is and then waits.
return {
    send = function()
        zyt.progress.share(40)
        zyt.time.sleep(600000)
    end,
}
