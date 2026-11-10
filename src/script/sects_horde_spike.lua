-- Sects & Cults — horde/regionless spike
-- On the first tick of a new campaign, gives away every region the player owns and
-- keeps the faction alive with the game's own "Regionless Faction" free-force bundle.
-- This tests whether a 0-region player faction survives and plays like a horde.

local SPIKE = {}

local function log(message)
    local line = "[sects-horde] " .. tostring(message)
    if output then pcall(output, line) end
    if out then pcall(out, line) end
    if se and se.log then pcall(se.log, line) end
end

local function gift_region(region_key, faction_key)
    return pcall(function()
        local region = cm:query_model():world():region_manager():region_by_key(region_key)
        cm:modify_model():get_modify_region(region):settlement_gifted_as_if_by_payload(
            cm:modify_faction(faction_key))
    end)
end

local function general_cqi()
    local ok, cqi = pcall(function()
        local faction = cm:query_faction(cm:get_local_faction())
        local characters = faction:character_list()
        for i = 0, characters:num_items() - 1 do
            local character = characters:item_at(i)
            local force = character:military_force()
            if force and not force:is_null_interface() then
                return character:cqi()
            end
        end
        return nil
    end)
    if ok then return cqi end
    return nil
end

function SPIKE.recruit(count, reason)
    if not (se and se.modify and se.modify.recruit) then
        log("recruit skipped: script extender recruit API unavailable")
        return
    end
    local cqi = general_cqi()
    if not cqi then
        log("recruit skipped: no general with an army")
        return
    end
    local units = { "3k_main_unit_metal_jian_swordguards", "3k_main_unit_metal_axe_band" }
    for i = 1, count do
        local unit = units[((i - 1) % #units) + 1]
        local call_ok, r1, r2 = pcall(se.modify.recruit, cqi, unit, { source = "any", free = true })
        local ok, message
        if call_ok then ok, message = r1, r2 else ok, message = false, r1 end
        log("recruit " .. reason .. ": " .. unit .. " -> " .. tostring(ok) .. " " .. tostring(message))
    end
end

function SPIKE.probe_api()
    local function collect(source, label)
        if type(source) ~= "table" then
            log(label .. " is " .. type(source) .. ", not a table")
            return
        end
        local hits = {}
        for key, value in pairs(source) do
            if type(value) == "function" and type(key) == "string"
                and (key:find("character") or key:find("force") or key:find("name")
                    or key:find("create") or key:find("spawn") or key:find("horde")
                    or key:find("region")) then
                hits[#hits + 1] = key
            end
        end
        table.sort(hits)
        log(label .. ": " .. table.concat(hits, ", "))
    end

    collect(cm, "cm functions")
    local mt = getmetatable(cm)
    if type(mt) == "table" and type(mt.__index) == "table" then
        collect(mt.__index, "cm methods")
    end
    for _, name in ipairs({ "create_character", "create_force_with_general", "create_character_from_template",
                            "spawn_character", "spawn_character_from_template", "create_force", "is_horde" }) do
        log("cm." .. name .. " = " .. type(cm and cm[name]))
    end
end

function SPIKE.make_regionless()
    local faction_key = cm:get_local_faction()
    local faction = cm:query_faction(faction_key)
    local regions = faction:region_list()
    local before = regions:num_items()
    log("faction " .. tostring(faction_key) .. " regions before: " .. before)
    log("is_horde before: " .. tostring(faction:is_horde()))

    local keys = {}
    for i = 0, before - 1 do
        keys[#keys + 1] = regions:item_at(i):name()
    end

    for _, key in ipairs(keys) do
        local ok, err = gift_region(key, "3k_main_faction_rebels")
        if ok then
            log("gifted " .. key .. " to rebels")
        else
            log("FAILED to gift " .. key .. ": " .. tostring(err))
        end
    end

    local after = cm:query_faction(faction_key):region_list():num_items()
    pcall(cm.apply_effect_bundle, cm, "3k_dlc04_effect_bundle_free_force", faction_key, 0)
    pcall(cm.apply_effect_bundle, cm, "sects_regionless_replenish", faction_key, 0)
    log("regionless bundles applied (free force + optional replenishment)")
    SPIKE.recruit(2, "first tick")
    log("regions after: " .. after .. ", is_horde after: " .. tostring(cm:query_faction(faction_key):is_horde()))
end

cm:add_first_tick_callback(function()
    local done = false
    pcall(function() done = cm:load_named_value("sects_horde_spike_done", "") == "yes" end)
    if done then
        log("already ran in this save; skipping")
        return
    end
    pcall(function() cm:save_named_value("sects_horde_spike_done", "yes") end)
    local ok, err = pcall(SPIKE.make_regionless)
    if not ok then log("spike failed: " .. tostring(err)) end
end)

core:add_listener(
    "sects_horde_spike_turn_start",
    "FactionTurnStart",
    function(context)
        return context:faction():name() == cm:get_local_faction()
    end,
    function(context)
        local faction = context:faction()
        local count = faction:region_list():num_items()
        log("turn start: regions=" .. count .. ", is_horde=" .. tostring(faction:is_horde()))
        if count == 0 then
            pcall(cm.apply_effect_bundle, cm, "3k_dlc04_effect_bundle_free_force", faction:name(), 0)
            pcall(cm.apply_effect_bundle, cm, "sects_regionless_replenish", faction:name(), 0)
            SPIKE.recruit(1, "turn start")
        end
    end,
    true)

cm:add_first_tick_callback(function()
    log("spike loaded")
    local ok, err = pcall(SPIKE.probe_api)
    if not ok then log("API probe failed: " .. tostring(err)) end
end)

log("file executing")
return SPIKE
