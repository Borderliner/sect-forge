-- The original faction and the new faction can both run the same campaign module.
local faction_key = "__FACTION_KEY__"
local base_cm, base_core = cm, core
local function active()
    if base_cm:is_multiplayer() then return false end
    local owner = base_cm:query_faction(faction_key)
    return owner and not owner:is_null_interface() and not owner:is_dead() and owner:is_human()
end

local function scoped(base, overrides)
    return setmetatable(overrides, {__index = function(_, name)
        local member = base[name]
        if type(member) == "function" then
            return function(_, ...) return member(base, ...) end
        end
        return member
    end})
end

local function guarded(callback)
    return function(...) if active() then return callback(...) end end
end

local cm = scoped(base_cm, {
    add_first_tick_callback = function(_, callback)
        return base_cm:add_first_tick_callback(guarded(callback))
    end,
    add_first_tick_callback_new = function(_, callback)
        return base_cm:add_first_tick_callback_new(guarded(callback))
    end,
    set_saved_value = function(_, key, value, group)
        return base_cm:set_saved_value(faction_key .. "_" .. key, value, group)
    end,
    get_saved_value = function(_, key, group)
        return base_cm:get_saved_value(faction_key .. "_" .. key, group)
    end,
    saved_value_exists = function(_, key, group)
        return base_cm:saved_value_exists(faction_key .. "_" .. key, group)
    end,
    save_named_value = function(_, key, value)
        return base_cm:save_named_value(faction_key .. "_" .. key, value)
    end,
    load_named_value = function(_, key, default)
        return base_cm:load_named_value(faction_key .. "_" .. key, default)
    end,
})

local core = scoped(base_core, {
    add_listener = function(_, name, event, test, callback, persistent)
        return base_core:add_listener(faction_key .. "_" .. name, event, function(context)
            return active() and (test == true or type(test) == "function" and test(context))
        end, callback, persistent)
    end,
    remove_listener = function(_, name)
        return base_core:remove_listener(faction_key .. "_" .. name)
    end,
    add_cli_listener = function() end,
})
