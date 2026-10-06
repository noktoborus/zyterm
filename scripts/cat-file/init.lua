-- A file onto a device that has nothing but `cat`.
--
-- It is the plainest transfer there is: the device is told to read the line
-- into a file and this end writes the file into the line. Nothing says when it
-- is over, so the end is a `ctrl+c` once the line is empty -- which is why
-- this is for a device with nothing better, and why what it carries is
-- believed rather than checked.

return {

    send = function()
        local path = zyt.target.path()
        zyt.line.type(string.format("cat > %s", zyt.shell.quote_posix(zyt.target.filename())))
        zyt.time.sleep(700)

        local program = zyt.proc.spawn{ command = "cat", args = { path } }
        local carrying = program:attach_to_line{ stderr = "notice" }
        local code = carrying:wait(600000)

        if code and code ~= 0 then
            error(string.format("cat ended with %d", code), 0)
        end
        zyt.notice.info(string.format(
            "%s went into the line; nothing on the device says whether it is whole",
            zyt.target.filename()
        ))
    end,
}
