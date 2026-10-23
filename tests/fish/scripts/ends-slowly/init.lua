-- A script that says it ends, and then takes its time.
return {
    send = function()
        zyt.run.ending()
        zyt.time.sleep(600000)
    end,
}
