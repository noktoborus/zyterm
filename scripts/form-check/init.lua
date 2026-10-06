-- Every kind of field a dialog has, in one window.
--
-- A window is checked by looking at it, and every change to `src/ui/form.rs`
-- is a change to how these rows are drawn: the width a field takes, the plate
-- a list opens on, the column of boxes to tick, the row that ends the window.
-- So the window is the whole of what this script does — nothing is sent, no
-- line is held, and what it was answered is printed in the terminal.
--
-- It is a script and not a page of the settings because the dialog belongs to
-- the scripts: it is described in Lua, and what describes it is what should
-- draw it here.

-- The entries of a list, long enough that a column of them is worth a look.
local FILES = {
    { "one.txt", "one.txt  12 B" },
    { "two.bin", "two.bin  3.4 K", "the second of them, as the pointer reads it" },
    { "three with spaces in its name.tar.gz", "three with spaces in its name.tar.gz  1.2 M" },
    { "four", "four/" },
    { "five.log", "five.log  128 K" },
}

-- The rows, in the order a person reads them.
local FIELDS = {
    {
        name = "line",
        kind = "text",
        label = "One line of text",
        value = "as it was typed",
        hint = "it grows with what is written in it, up to the room of the window",
        required = true,
    },
    {
        name = "secret",
        kind = "text",
        password = true,
        label = "A line nothing shows",
        value = "nothing to see",
    },
    {
        name = "lines",
        kind = "textarea",
        rows = 2,
        label = "Text of several lines",
        value = "the first line\nthe second",
    },
    {
        name = "switch",
        kind = "switch",
        label = "A switch",
        value = true,
    },
    { kind = "separator" },
    {
        name = "note",
        kind = "note",
        label = "A line that asks nothing, which is what this one is",
    },
    {
        name = "one",
        kind = "one_of",
        label = "One of several",
        value = "base64",
        options = {
            { "base64", "base64" },
            { "raw", "raw, with stty", "what an entry says about itself" },
        },
    },
    {
        name = "picked",
        kind = "select",
        label = "One picked from a list, or none",
        options = {
            { "none", "do not compare" },
            { "sha256sum", "sha256sum" },
            { "md5sum", "md5sum" },
        },
    },
    {
        name = "many",
        kind = "many_of",
        label = "Any number of a list",
        options = FILES,
    },
    { kind = "separator" },
    {
        name = "measured",
        kind = "text",
        label = "A label long enough to measure the column of labels with",
        value = "",
    },
}

-- What one answer reads as in the terminal.
local function text_of(value)
    if type(value) == "table" then
        if #value == 0 then
            return "nothing"
        end
        return table.concat(value, ", ")
    end
    if value == nil then
        return "nothing"
    end
    return tostring(value)
end

return {

    send = function()
        local answers = zyt.ui.ask{
            id = "widgets",
            title = "Every kind of field",
            hint = "Nothing here is carried anywhere: the window is the subject",
            accept = "Say what it answered",
            fields = FIELDS,
        }
        if not answers then
            zyt.notice.info("the window was waved away, which is an answer too")
            return
        end

        for _, field in ipairs(FIELDS) do
            if field.name and field.kind ~= "note" then
                zyt.term.print(string.format("%s = %s", field.name, text_of(answers[field.name])))
            end
        end
        zyt.notice.info("every field answered, and nothing was sent")
    end,
}
