-- Sects & Cults — M0 smoke test
-- Registers a first-tick handler that logs through the game's own output channel
-- and applies the test belief bundle to the local faction for 3 turns.

local SECTS = { fired = false }

local function log(message)
    local line = "[sects] " .. tostring(message)
    if output then pcall(output, line) end
    if out then pcall(out, line) end
    if se and se.log then pcall(se.log, line) end
end

function SECTS.se_status()
    if not se then return "script extender library not present" end
    local ok, status = pcall(se.status)
    if not ok or not status then return "se.status() failed" end
    return string.format("state=%s version=%s patches=%s",
        tostring(status.state), tostring(status.version), tostring(status.patches))
end

function SECTS.on_first_tick()
    if SECTS.fired then return end
    SECTS.fired = true

    log("M0 loaded - " .. SECTS.se_status())

    local ok, faction = pcall(function() return cm:get_local_faction() end)
    if not ok or not faction then
        log("no local faction yet; skipping bundle test")
        return
    end

    log("local faction: " .. tostring(faction))
    local applied, err = pcall(cm.apply_effect_bundle, cm, "sects_m0_bundle", faction, 3)
    if applied then
        log("applied sects_m0_bundle for 3 turns")
    else
        log("apply_effect_bundle failed: " .. tostring(err))
    end
end

log("file executing")

if cm and cm.add_first_tick_callback then
    local ok, err = pcall(cm.add_first_tick_callback, cm, SECTS.on_first_tick)
    log("first-tick callback registered=" .. tostring(ok) .. (ok and "" or (" err=" .. tostring(err))))
else
    log("cm:add_first_tick_callback is unavailable")
end

if core and core.add_listener then
    local ok, err = pcall(core.add_listener, core, "sects_first_tick", "FirstTickAfterWorldCreated",
        true, function() SECTS.on_first_tick() end, false)
    log("listener registered=" .. tostring(ok) .. (ok and "" or (" err=" .. tostring(err))))
end

return SECTS
