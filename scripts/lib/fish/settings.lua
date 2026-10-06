-- What a transfer is asked before it runs.
--
-- Every one of these was a flag of a command line, and every one of them is a
-- field now: the mode is settled before the first byte because the probe asks
-- for what that mode needs; the chunk is the size on the wire; the sum is a
-- policy and not only a name; and the measurement at the end is what catches a
-- chunk that never landed.

local catalog = require("fish.catalog")

local settings = {}

-- The sums the device may be asked for, strongest first.
settings.DIGESTS = { "sha256sum", "sha1sum", "md5sum" }

-- Asks what the transfer is to do.
--
-- `how` says which direction is being asked about: a transfer that takes a
-- whole directory off the device has one more question than one that puts
-- files onto it.
function settings.ask(how)
    how = how or {}

    local fields = {
        {
            name = "mode",
            kind = "one_of",
            label = "On the wire",
            value = "base64",
            hint = "base64 crosses any line; raw is a third faster and needs stty on the device",
            options = { { "base64", "base64" }, { "raw", "raw, with stty" } },
        },
        {
            name = "chunk",
            kind = "text",
            label = "Chunk, bytes of the line",
            value = tostring(how.chunk or catalog.DEFAULT_CHUNK),
            hint = "a small chunk says where a transfer stands more often and costs a round trip for it",
        },
        {
            name = "digest",
            kind = "select",
            label = "Compare sums",
            value = "auto",
            options = {
                { "none", "do not compare" },
                { "auto", "the strongest the device has" },
                { "sha256sum", "sha256sum" },
                { "sha1sum", "sha1sum" },
                { "md5sum", "md5sum" },
            },
        },
        {
            name = "size_check",
            kind = "switch",
            label = "Measure the file on the device afterwards",
            value = true,
        },
        {
            name = "timeout",
            kind = "text",
            label = "The device may be quiet for, seconds",
            value = tostring(how.timeout or 30),
        },
        {
            name = "remote",
            kind = "text",
            label = how.remote_label or "On the device",
            value = how.remote or ".",
            required = true,
        },
    }

    if how.with_all then
        fields[#fields + 1] = {
            name = "all",
            kind = "switch",
            label = "Take everything under it",
            value = true,
        }
    end

    fields[#fields + 1] = {
        name = "verbose",
        kind = "switch",
        label = "Say every command in the terminal",
        value = false,
    }

    local answers = zyt.ui.ask{
        title = how.title or "Shell transfer",
        hint = how.hint,
        accept = how.accept,
        fields = fields,
    }
    if not answers then
        return nil
    end

    return {
        mode = answers.mode,
        chunk = math.max(math.tointeger(tonumber(answers.chunk) or 0) or catalog.DEFAULT_CHUNK, 1),
        digest = answers.digest,
        size_check = answers.size_check and true or false,
        timeout = math.max((math.tointeger(tonumber(answers.timeout) or 0) or 30) * 1000, 1000),
        remote = answers.remote,
        all = answers.all and true or false,
        verbose = answers.verbose and true or false,
    }
end

-- Which sum is going to be compared, after asking the device.
--
-- `auto` tries them strongest first and carries on without comparing when the
-- device has none; one named outright is refused rather than carried
-- unchecked, because somebody who asked for a sum asked for the answer.
function settings.digest_for(session, wanted)
    if wanted == nil or wanted == "none" then
        return nil
    end

    if wanted == "auto" then
        for _, program in ipairs(settings.DIGESTS) do
            if session:offers_digest(program) then
                return program
            end
        end
        zyt.notice.info("the device can say no sum, so nothing is compared")
        return nil
    end

    if not session:offers_digest(wanted) then
        error(string.format("the device has no working %s", wanted), 0)
    end
    return wanted
end

-- The name of the sum as this machine takes it.
function settings.summing(program)
    return (string.gsub(program, "sum$", ""))
end

return settings
