-- A script that writes back whatever the device said.
return {
    send = function()
        while true do
            local said = zyt.line.read(600000)
            if said then
                zyt.line.write(said)
            end
        end
    end,
}
