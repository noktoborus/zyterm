-- Waits for a timer, then the line, then a client, then room to write to it,
-- each with one zyt.wait; then waits long, to be cancelled.
return {
    send = function()
        local port = zyt.net.listen{ host = "127.0.0.1", port = 0 }
        zyt.term.print("at " .. port:address() .. " ")

        local ready = zyt.wait{ timeout = 50 }
        zyt.term.print(ready.timeout and "[timer]" or "[no timer]")

        ready = zyt.wait{ line = true, sockets = { port }, timeout = 5000 }
        zyt.term.print(ready.line and "[line]" or "[no line]")
        zyt.line.read(0)

        ready = zyt.wait{ line = true, sockets = { port }, timeout = 5000 }
        zyt.term.print(ready[port] and "[client]" or "[no client]")
        local client = port:accept(0)

        ready = zyt.wait{ writable = { client }, timeout = 5000 }
        zyt.term.print(ready.writable[client] and "[writable]" or "[not writable]")

        zyt.term.print("[long]")
        zyt.wait{ timeout = 600000 }
    end,
}
