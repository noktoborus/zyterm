-- A script that asks one thing, and goes on waiting whatever it is told.
return {
    send = function()
        zyt.ui.ask{
            id = "asked",
            unsaved = true,
            title = "Asked by a script",
            fields = {
                { name = "what", kind = "text", label = "What" },
            },
        }
        zyt.time.sleep(600000)
    end,
}
