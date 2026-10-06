-- What a script does with the line, the terminal and the bar.
return {

    send = function()
        zyt.line.type("pwd")
        local said = zyt.line.read(2000)
        zyt.term.print("said " .. (said or "nothing"))
        zyt.notice.info("the device answered")
        zyt.progress.share(42)
        zyt.line.write_key("enter")
    end,
}
