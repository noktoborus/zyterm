-- Listens on a port of its own choosing, says which, answers one client and
-- refuses a second; then either ends or fails with both still open, which is
-- what the end of the run has to close.

return {
    send = function()
        local port = zyt.net.listen{ host = "127.0.0.1", port = 0 }
        zyt.term.print("at " .. port:address())

        local client = port:accept(5000)
        if not client then
            error("nobody connected", 0)
        end
        local second = port:accept(2000)
        if second then
            second:write("busy\r\n")
            second:close()
        end

        local came, why = client:read(5000)
        client:write("got " .. tostring(came))

        if came == "fail" then
            error("failed with the port open", 0)
        end
        came, why = client:read(5000)
        zyt.term.print("then " .. tostring(came) .. " " .. tostring(why))
    end,
}
