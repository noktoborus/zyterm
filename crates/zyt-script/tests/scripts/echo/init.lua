-- What a script does with the line, the terminal and the bar.
return {

    send = function()
        zyt.line.type("pwd")
        -- The echo may come a piece at a time: what one read gives is what
        -- has come so far, so it is read until it holds the word.
        local said = ""
        for _ = 1, 20 do
            said = said .. (zyt.line.read(100) or "")
            if said:find("pwd", 1, true) then
                break
            end
        end
        zyt.term.print("said " .. (said ~= "" and said or "nothing"))
        zyt.notice.info("the device answered")
        zyt.progress.share(42)
        zyt.line.write_key("enter")
    end,
}
