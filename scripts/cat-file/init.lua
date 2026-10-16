-- A file onto a device that has nothing but `cat`.
--
-- It is the plainest transfer there is: the device is told to read the line
-- into a file and this end writes the file into the line. Nothing says when it
-- is over, so the end is a `ctrl+c` once the line is empty -- which is why
-- this is for a device with nothing better, and why what it carries is
-- believed rather than checked.

-- Writes one file into the line, under its own name on the device, and
-- interrupts `cat` there once the line is empty: nothing else ends it.
local function send(path)
    local name = path:match("([^/\\]+)$") or path
    zyt.line.type(string.format("cat > %s", zyt.shell.quote_posix(name)))
    zyt.time.sleep(700)

    local program = zyt.proc.spawn{ command = "cat", args = { path } }
    local carrying = program:attach_to_line{ stderr = "notice" }
    local code = carrying:wait(600000)
    zyt.line.wait_drained()
    zyt.line.write_key("ctrl+c")

    if code and code ~= 0 then
        error(string.format("cat ended with %d", code), 0)
    end
    zyt.notice.info(string.format(
        "%s went into the line; nothing on the device says whether it is whole",
        name
    ))
end

return {

    send = function()
        local picked = zyt.ui.pick{ kind = "file", title = "Cat file: what goes onto the device" }
        if picked then
            send(picked[1])
        end
    end,

    -- One file at a time: `cat` on the far end writes one.
    drop = function()
        local paths = zyt.target.paths()
        if #paths ~= 1 then
            error("cat carries one file at a time", 0)
        end
        send(paths[1])
    end,

    -- A run stopped midway leaves `cat` reading on the device; the key goes
    -- behind whatever is still queued, since nothing may be waited for here.
    cleanup = function()
        zyt.line.write_key("ctrl+c")
    end,
}
