-- What a walk of the device carries.
--
-- A symbolic link is never carried as a link: it is followed and what it
-- leads to is carried in its place, which is what the tests of the device
-- already do. A link that leads back into its own tree would make the walk
-- endless, so the real path of every directory is asked for and remembered,
-- and a walk deeper than this is refused for a device whose `pwd -P` cannot
-- answer.

local walk = {}

-- How many directories deep a walk of the device goes.
walk.MAX_DEPTH = 64

-- The name of a path, without the directories above it.
local function named(path)
    return (string.match(path, "([^/]+)/*$")) or path
end

-- One path joined under another, with the one slash between them.
local function joined(above, below)
    if above == "" then
        return below
    end
    return (string.gsub(above, "/+$", "")) .. "/" .. below
end

local function walk_into(session, directory, under, seen, items, depth)
    if depth > walk.MAX_DEPTH then
        error(string.format("%s is deeper than %d directories", directory, walk.MAX_DEPTH), 0)
    end

    local real = session:canonical(directory)
    if real ~= "" then
        if seen[real] then
            return
        end
        seen[real] = true
    end

    for _, entry in ipairs(session:list(directory)) do
        local path = joined(directory, entry.name)
        local relative = joined(under, entry.name)
        if entry.kind == "file" then
            items[#items + 1] = { path = path, relative = relative, size = entry.size }
        elseif entry.kind == "directory" then
            walk_into(session, path, relative, seen, items, depth + 1)
        end
    end
end

-- Every file under the named paths of the device, with where each belongs.
function walk.remote(session, roots)
    local items = {}
    local seen = {}

    for _, root in ipairs(roots) do
        local kind = session:kind(root)
        if kind == "file" then
            items[#items + 1] = {
                path = root,
                relative = named(root),
                size = session:size(root),
            }
        elseif kind == "directory" then
            walk_into(session, root, named(root), seen, items, 1)
        else
            zyt.notice.info(string.format("%s is neither a file nor a directory", root))
        end
    end

    return items
end

-- The directory part of a path, or nothing when it names no directory.
function walk.above(path)
    local above = string.match(path, "^(.*)/[^/]+/*$")
    if above == "" then
        return "/"
    end
    return above
end

return walk
