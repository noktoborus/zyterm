-- A script that types a command into the device and waits.
return {
    send = function()
        zyt.line.type("cat > 'payload.bin'")
        zyt.time.sleep(600000)
    end,
}
