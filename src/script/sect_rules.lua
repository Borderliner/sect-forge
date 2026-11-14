-- Influence is faction-owned and survives migration and save/load.
local faction_key = "__FACTION_KEY__"
local resource_key = faction_key .. "_influence"
local factor_key = faction_key .. "_teachings"
local setback_factor = faction_key .. "_setbacks"
local doctrines = { __DOCTRINE_GAINS__ }
local thresholds = { __INFLUENCE_TIERS__ }
local influence_cap = __INFLUENCE_CAP__
local starting_traits = { __STARTING_TRAITS__ }
local starting_ancillaries = { __STARTING_ANCILLARIES__ }
local starting_region = "__START_REGION__"
local start_preplaced = __START_PREPLACED__
local starting_owner = "__START_OWNER__"
local starting_building = "__START_BUILDING__"
local sect_mode = __SECT_MODE__
local is_cult = __IS_CULT__
local hepu = "3k_main_hepu_capital"
local base_value = faction_key .. "_base"
local growth_turn = faction_key .. "_growth_turn"
local chapters_value = faction_key .. "_chapters"
local chapter_cap = 5
local assembly_key = faction_key .. "_assembly"
local donations_key = faction_key .. "_assembly_donations"
local dispatch_key = faction_key .. "_patron_dispatch"
local relief_key = faction_key .. "_household_relief"
local unfunded_relief_key = relief_key .. "_unfunded"
local contacts_value = faction_key .. "_initial_contacts"
local reconciliation_queued = false
local placing_start = false

local function faction()
    return cm:query_faction(faction_key)
end

local function prepare_opening_army(owner, reposition)
    if not owner:has_capital_region() or not owner:has_faction_leader() then return end
    local leader = owner:faction_leader()
    if not leader:has_military_force() or (not reposition and leader:action_points_remaining_percent() > 0) then return end

    -- The BOB template places the leader by Hepu but can leave the army unable to move.
    local army = cm:modify_character(leader)
    army:enable_movement()
    local found, x, y = owner:get_valid_spawn_location_in_region(owner:capital_region():name(), false)
    if found then army:teleport_to(x, y) end
    army:replenish_action_points()
end

local function place_starting_city(owner)
    local target = cm:query_region(starting_region)
    local original_owner = cm:query_faction("3k_main_faction_shi_xie")
    if not target or target:is_null_interface() or not owner:has_capital_region()
        or owner:capital_region():name() ~= hepu or not owner:has_faction_leader()
        or not original_owner or original_owner:is_null_interface() or original_owner:is_dead() then
        script_error("Sect Forge cannot establish its starting city at " .. starting_region)
        prepare_opening_army(owner)
        return
    end
    local target_owner = target:owning_faction()
    if not target_owner or target_owner:is_null_interface() or target_owner:name() ~= starting_owner
        or (starting_owner == "rebels" and not target:building_exists(starting_building))
        or (starting_owner ~= "rebels" and target_owner:has_capital_region()
            and target_owner:capital_region():name() == starting_region) then
        script_error("Sect Forge cannot take the starting city at " .. starting_region)
        prepare_opening_army(owner)
        return
    end
    local leader = owner:faction_leader()
    local found, x, y = owner:get_valid_spawn_location_in_region(starting_region, false)
    if not found or not leader:has_military_force() then
        script_error("Sect Forge cannot place its starting army at " .. starting_region)
        prepare_opening_army(owner)
        return
    end

    local old_capital = owner:capital_region()
    local modify_owner = cm:modify_faction(owner)
    placing_start = true
    cm:modify_region(target):settlement_gifted_as_if_by_payload(modify_owner)
    local function keep_hepu()
        if target:owning_faction():name() == faction_key then
            modify_owner:make_region_capital(old_capital)
            local modify_target = cm:modify_region(target)
            if starting_owner == "rebels" then
                modify_target:raze_and_abandon_settlement_without_attacking()
            else
                modify_target:settlement_gifted_as_if_by_payload(cm:modify_faction(target_owner))
            end
        end
        placing_start = false
        script_error("Sect Forge kept Hepu after failing to establish " .. starting_region)
        prepare_opening_army(owner)
    end
    if target:owning_faction():name() ~= faction_key or target:is_abandoned()
        or not modify_owner:make_region_capital(target) then
        keep_hepu()
        return
    end
    local army = cm:modify_character(leader)
    army:enable_movement()
    army:teleport_to(x, y)
    cm:callback(function()
        if not leader:has_region() or leader:region():name() ~= starting_region then
            keep_hepu()
            return
        end
        army:replenish_action_points()
        cm:modify_region(old_capital):settlement_gifted_as_if_by_payload(cm:modify_faction(original_owner))
        cm:set_saved_value(base_value, starting_region)
        placing_start = false
        if not cm:is_multiplayer() and cm:get_local_faction() == faction_key then
            local _, _, distance, bearing, height = cm:get_camera_position()
            cm:set_camera_position(leader:display_position_x(), leader:display_position_y(), distance, bearing, height)
        end
    end, 0.5)
end

local function equip_starting_ancillaries(owner)
    if #starting_ancillaries == 0 or not owner:has_faction_leader() then return end
    local leader = owner:faction_leader()
    local inventory = cm:modify_faction(owner):ceo_management()
    local personal_inventory = cm:modify_character(leader):ceo_management()
    for _, item in ipairs(starting_ancillaries) do
        local key, category, character_owned, permission = item[1], item[2], item[3], item[4]
        if permission ~= "" then personal_inventory:add_scripted_permission(permission) end
        local slot = ancillaries:get_valid_slot_for_category(leader, category)
        local equipped = slot and slot:equipped_ceo()
        if not equipped or equipped:is_null_interface() or equipped:ceo_data_key() ~= key then
            local source = character_owned and personal_inventory or inventory
            source:add_ceo(key)
            ancillaries:equip_ceo_on_character(leader, key, category)
            local applied = slot and slot:equipped_ceo()
            if not applied or applied:is_null_interface() or applied:ceo_data_key() ~= key then
                output("[SectForge] Could not equip starting ancillary " .. key .. " for " .. faction_key)
            end
        end
    end
end

local function set_starting_traits(owner)
    if #starting_traits == 0 or not owner:has_faction_leader() then return end
    local leader = owner:faction_leader()
    local query = leader:ceo_management()
    local modify = cm:modify_character(leader):ceo_management()
    local wanted, present, remove = {}, {}, {}
    for _, key in ipairs(starting_traits) do wanted[key] = true end
    for _, kind in ipairs({"personality", "physical"}) do
        local traits = query:all_ceos_for_category("3k_main_ceo_category_traits_" .. kind)
        for i = 0, traits:num_items() - 1 do
            local key = traits:item_at(i):ceo_data_key()
            if wanted[key] then present[key] = true else remove[#remove + 1] = key end
        end
    end
    -- Removing a CEO invalidates iterators over the character's CEO lists.
    for _, key in ipairs(remove) do modify:remove_ceos(key) end
    for _, key in ipairs(starting_traits) do
        if not present[key] then modify:add_ceo(key) end
    end
    local applied = {}
    local all = query:all_ceos()
    for i = 0, all:num_items() - 1 do
        local ceo = all:item_at(i)
        if wanted[ceo:ceo_data_key()] and ceo:current_node_key() ~= "" then
            applied[#applied + 1] = ceo:ceo_data_key()
        end
    end
    if #applied ~= #starting_traits then
        script_error("Sect Forge could not activate all starting traits for " .. faction_key)
    end
    output("[SectForge] " .. faction_key .. " starting traits=" .. table.concat(applied, ", "))
end

local function update_influence_effects()
    local owner = faction()
    local resource = owner:pooled_resources():resource(resource_key)
    if resource:is_null_interface() then
        script_error("Sect Forge could not find " .. resource_key)
        return
    end
    local value = resource:value()
    local tier = 0
    for _, threshold in ipairs(thresholds) do
        if value >= threshold then tier = threshold end
    end
    for _, threshold in ipairs(thresholds) do
        local bundle = faction_key .. "_influence_" .. threshold
        local active = owner:has_effect_bundle(bundle)
        if tier == threshold and not active then
            cm:apply_effect_bundle(bundle, faction_key, -1)
        elseif tier ~= threshold and active then
            cm:remove_effect_bundle(bundle, faction_key)
        end
    end
end

local function add_influence(owner, amount)
    local resource = owner:pooled_resources():resource(resource_key)
    if resource:is_null_interface() then return end
    local value = resource:value()
    amount = math.max(-value, math.min(math.max(0, influence_cap - value), amount))
    if amount == 0 then return end
    local factor = amount < 0 and setback_factor or factor_key
    cm:modify_model():get_modify_pooled_resource(resource):apply_transaction_to_factor(factor, amount)
    update_influence_effects()
end

local function current_patron()
    local patron_key = cm:get_saved_value(faction_key .. "_patron")
    if not patron_key then return end
    local patron = cm:query_faction(patron_key)
    if patron and not patron:is_null_interface() and not patron:is_dead() then return patron end
end

local function restore_founder_family_chance(owner)
    if not owner:is_human() or not owner:has_faction_leader() then return end
    local founder = owner:faction_leader()
    if founder:generation_template_key() ~= faction_key .. "_leader"
        or founder:is_male() or not founder:has_effect_bundle("less_child") then return end
    local family = founder:family_member()
    if family:is_null_interface() or not family:has_spouse() then return end
    local spouse = family:spouse():character()
    if not spouse:is_null_interface() and not spouse:is_dead() and spouse:is_male()
        and spouse:faction():name() == faction_key then
        -- Marriage Harem Mod Reloaded reapplies this penalty to female player
        -- leaders. Restore the regular marriage rules for this founder only.
        cm:modify_character(founder):remove_effect_bundle("less_child")
    end
end

local function establish_initial_contacts(owner)
    if not owner:is_human() or cm:get_saved_value(contacts_value) then return end
    local factions = cm:query_model():world():faction_list()
    local met = owner:factions_met()
    local known = {}
    for i = 0, met:num_items() - 1 do known[met:item_at(i):name()] = true end
    local envoy = cm:modify_faction(owner)
    for i = 0, factions:num_items() - 1 do
        local other = factions:item_at(i)
        local key = other:name()
        if key ~= faction_key and not known[key] and not other:is_dead() and not other:is_rebel()
            and other:has_capital_region() then
            -- Revealing the seat establishes contact through the campaign's visibility system.
            envoy:make_region_visible_in_shroud(other:capital_region():name())
        end
    end
    cm:set_saved_value(contacts_value, true)
    cm:callback(function()
        output("[SectForge] " .. faction_key .. " factions met=" .. owner:factions_met():num_items())
    end, 1)
end

local function reconcile_base()
    if not sect_mode or placing_start then return end
    local owner = faction()
    if not owner or owner:is_null_interface() or owner:is_dead() then return end
    local regions = owner:region_list()
    if regions:num_items() == 0 then return end
    local preferred = cm:get_saved_value(base_value)
    local commandery_by_region, region_keys, seats = {}, {}, {}
    for index = 0, regions:num_items() - 1 do
        local region = regions:item_at(index)
        local key, commandery = region:name(), region:province_name()
        commandery_by_region[key] = commandery
        region_keys[#region_keys + 1] = key
        if region:is_province_capital() then seats[commandery] = key end
    end
    local capital = owner:has_capital_region() and owner:capital_region():name()
    local anchor = commandery_by_region[preferred] and preferred
        or commandery_by_region[capital] and capital or region_keys[1]
    local keep_commandery = commandery_by_region[anchor]
    local keep = commandery_by_region[capital] == keep_commandery and capital or seats[keep_commandery] or anchor
    if capital ~= keep then
        if not cm:modify_faction(faction_key):make_region_capital(cm:query_region(keep)) then
            script_error("Sect Forge could not move the capital to " .. keep .. "; retrying next turn")
            return
        end
    end
    cm:set_saved_value(base_value, keep)
    -- Snapshot the keys before mutation, since abandonment changes the region list.
    for _, region_key in ipairs(region_keys) do
        if commandery_by_region[region_key] ~= keep_commandery then
            local region = cm:query_region(region_key)
            local current_owner = region:owning_faction()
            if current_owner and not current_owner:is_null_interface() and current_owner:name() == faction_key then
                if not cm:modify_region(region):raze_and_abandon_settlement_without_attacking() then
                    script_error("Sect Forge could not release " .. region_key .. "; retrying next turn")
                end
            end
        end
    end
end

local function queue_reconciliation(preferred)
    cm:set_saved_value(base_value, preferred)
    if reconciliation_queued then return end
    reconciliation_queued = true
    cm:callback(function()
        reconciliation_queued = false
        reconcile_base()
    end, 0.1)
end

local function grow_influence(context)
    reconcile_base()
    local owner = context:faction()
    local turn = cm:query_model():turn_number()
    if cm:get_saved_value(growth_turn) == turn then return end
    local resource = owner:pooled_resources():resource(resource_key)
    if resource:is_null_interface() then return end
    cm:set_saved_value(growth_turn, turn)
    if is_cult then
        add_influence(owner, -1)
        return
    end
    local chapters = cm:get_saved_value(chapters_value) or 0
    local gain = 1 + math.floor(turn * chapters / 3) - math.floor((turn - 1) * chapters / 3)
    for doctrine, amount in pairs(doctrines) do
        local technology = faction_key .. "_" .. doctrine
        local reward = technology .. "_influence_awarded"
        if owner:has_technology(technology) and not cm:get_saved_value(reward) then
            cm:set_saved_value(reward, true)
            gain = gain + amount
        end
    end
    if cm:get_saved_value(faction_key .. "_patron") and not current_patron() then
        cm:set_saved_value(faction_key .. "_patron", false)
        add_influence(owner, -10)
    end
    add_influence(owner, gain)
    if owner:is_human() and turn % 6 == 4 then
        local dispatch = turn % 12 == 10 and owner:has_technology(faction_key .. "_patrons")
            and owner:treasury() >= 1500 and current_patron()
        local can_endow = owner:treasury() >= 1000 and chapters < chapter_cap
        cm:trigger_dilemma(faction_key, dispatch and dispatch_key or can_endow and assembly_key or donations_key, true)
    elseif owner:is_human() and turn % 10 == 5 and resource:value() >= 5 then
        cm:trigger_dilemma(faction_key, owner:treasury() >= 600 and relief_key or unfunded_relief_key, true)
    end
    output("[SectForge] " .. faction_key .. " influence=" .. resource:value() .. " gain=" .. gain)
end

cm:add_first_tick_callback(function()
    local owner = faction()
    if not owner or owner:is_null_interface() or owner:is_dead() then return end
    if cm:is_new_game() and (start_preplaced or starting_region == hepu) then
        prepare_opening_army(owner, start_preplaced and starting_region ~= hepu)
    end
    reconcile_base()
    update_influence_effects()
    restore_founder_family_chance(owner)
    if not is_cult then establish_initial_contacts(owner) end
    -- Reloading the opening turn must not grant Influence a second time.
    if not cm:get_saved_value(growth_turn) then
        cm:set_saved_value(growth_turn, cm:query_model():turn_number())
    end
end)

cm:add_first_tick_callback_new(function()
    local owner = faction()
    if owner and not owner:is_null_interface() and not owner:is_dead() then
        set_starting_traits(owner)
        equip_starting_ancillaries(owner)
        cm:modify_faction(faction_key):set_tech_research_cooldown(0)
        if not start_preplaced and starting_region ~= hepu then place_starting_city(owner) end
    end
end)

core:add_listener(faction_key .. "_single_base", "RegionOwnershipChanged",
    function(context)
        local owner = context:new_owner()
        return sect_mode and not placing_start and owner and not owner:is_null_interface() and owner:name() == faction_key
    end,
    function(context) queue_reconciliation(context:region():name()) end, true)

core:add_listener(faction_key .. "_followers", "FactionTurnStart",
    function(context) return context:faction():name() == faction_key end,
    grow_influence, true)

core:add_listener(faction_key .. "_founder_family", "FactionAboutToEndTurn",
    function(context) return context:faction():name() == faction_key end,
    function(context) restore_founder_family_chance(context:faction()) end, true)

if is_cult then
    core:add_listener(faction_key .. "_raid_reputation", faction_key .. "_raid_completed", true,
        function() add_influence(faction(), 5) end, true)
    core:add_listener(faction_key .. "_contract_reputation", faction_key .. "_contract_expired", true,
        function() add_influence(faction(), -5) end, true)
else
    core:add_listener(faction_key .. "_lost_settlement", "RegionOwnershipChanged",
        function(context)
            local previous, next_owner = context:previous_owner(), context:new_owner()
            return not placing_start and previous and not previous:is_null_interface() and previous:name() == faction_key
                and next_owner and not next_owner:is_null_interface() and next_owner:name() ~= faction_key
                and not next_owner:is_rebel()
        end,
        function(context) add_influence(context:previous_owner(), -6) end, true)

    core:add_listener(faction_key .. "_household_choices", "DilemmaChoiceMadeEvent",
        function(context)
            local dilemma = context:dilemma()
            return context:faction():name() == faction_key
                and (dilemma == assembly_key or dilemma == donations_key or dilemma == dispatch_key
                    or dilemma == relief_key or dilemma == unfunded_relief_key)
        end,
        function(context)
            local owner = context:faction()
            local treasury = cm:modify_faction(owner)
            local dilemma = context:dilemma()
            if dilemma == relief_key or dilemma == unfunded_relief_key then
                if dilemma == relief_key and context:choice() == 0 and owner:treasury() >= 600 then
                    treasury:decrease_treasury(600)
                else
                    add_influence(owner, -5)
                end
                return
            end
            if dilemma == dispatch_key then
                local patron = current_patron()
                if context:choice() == 0 and patron and owner:treasury() >= 1500 then
                    treasury:decrease_treasury(1500)
                    cm:modify_faction(patron):increase_treasury(1500)
                end
                return
            end
            if context:choice() == 0 then
                treasury:increase_treasury(900)
            elseif context:choice() == 2 and dilemma == assembly_key then
                local chapters = cm:get_saved_value(chapters_value) or 0
                if chapters < chapter_cap and owner:treasury() >= 1000 then
                    treasury:decrease_treasury(1000)
                    cm:set_saved_value(chapters_value, chapters + 1)
                    add_influence(owner, 6)
                    local loc = "event_feed_strings_text_" .. faction_key .. "_chapter_"
                    treasury:show_message_event(loc .. "title", loc .. (chapters + 1), loc .. "body", true, 0)
                end
            end
        end, true)
end
