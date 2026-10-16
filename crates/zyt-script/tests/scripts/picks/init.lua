-- Asks the dialog of the file system for files, and says what it was given.
return {
    send = function()
        local paths = zyt.ui.pick{ kind = "files", title = "What goes" }
        if not paths then
            zyt.term.print("picked nothing")
            return
        end
        zyt.term.print("picked " .. table.concat(paths, ","))
    end,
}
