-- A script that says one thing in the frame of the application.
return {
    send = function()
        zyt.notice.info("Bytes Sent: 1024")
        zyt.time.sleep(600000)
    end,
}
