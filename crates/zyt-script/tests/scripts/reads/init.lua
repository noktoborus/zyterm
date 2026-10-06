-- A script whose shell lives in files of its own.
return {
    send = function()
        local names = zyt.script.list_files("sh")
        local template = zyt.script.read_file("sh/" .. names[1])
        local folded = zyt.script.fold(template)
        zyt.line.write_text(zyt.script.fill(folded, { path = zyt.shell.quote_posix("/tmp/two words") }))
        zyt.term.print("files " .. #names)
    end,
}
