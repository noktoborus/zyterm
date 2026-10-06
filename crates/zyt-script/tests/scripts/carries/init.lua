-- What the script was given to carry, and what it was told.
return {
    send = function()
        zyt.line.write_text(table.concat({
            zyt.target.kind,
            zyt.target.path(),
            zyt.target.filename(),
            zyt.target.stem(),
            zyt.target.suffix(),
            zyt.vars.require("remote_host"),
            zyt.codec.short_size(4096),
        }, "|"))
    end,
}
