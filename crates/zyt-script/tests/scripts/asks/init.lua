-- Every kind of field a dialog offers.
return {
    send = function()
        local answers = zyt.ui.ask{
            title = "Ask",
            fields = {
                { name = "host", kind = "text", value = "board" },
                { name = "notes", kind = "textarea", rows = 1 },
                { name = "verify", kind = "switch", value = true },
                { name = "mode", kind = "one_of", options = { { "b64", "base64" }, { "raw", "raw" } } },
                { name = "proto", kind = "select", options = { "fish", "zmodem" } },
                { name = "steps", kind = "many_of", options = { "sync", "reboot" }, value = { "sync" } },
                { kind = "separator" },
            },
        }
        if not answers then
            zyt.term.print("waved away")
            return
        end
        zyt.line.write_text(answers.host .. " " .. answers.mode .. " " .. tostring(answers.verify))
    end,
}
