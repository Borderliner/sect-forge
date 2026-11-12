local faction_key = "__FACTION_KEY__"
local is_cult = __IS_CULT__
local sack_target = __CULT_SACK_TARGET__
local victory_value = faction_key .. "_victory_achieved"
local patron_value = faction_key .. "_patron"
local sack_count = faction_key .. "_cities_sacked"
local contract_key = faction_key .. "_plunder_contract"
local contract_until = faction_key .. "_contract_until"
local next_contract = faction_key .. "_next_contract"
local heat_key = faction_key .. "_heat"

local function player_faction()
    local owner = cm:query_faction(faction_key)
    return owner and not owner:is_null_interface() and owner:is_human() and not owner:is_dead()
end

local function award_victory()
    if not player_faction() or cm:get_saved_value(victory_value) then return end
    cm:set_saved_value(victory_value, true)
    progression:force_campaign_victory(faction_key)
end

local function check_patron(patron)
    if patron:name() == cm:get_saved_value(patron_value) and patron:is_world_leader() then
        award_victory()
    end
end

local function sponsored_warlord(deal)
    local terms = deal:components()
    if terms:num_items() ~= 1 then return end
    local payment = terms:item_at(0)
    if payment:treaty_component_key() ~= "treaty_components_payment_regular_offer" then return end
    local donor, patron = payment:proposer(), payment:recipient()
    if donor:is_null_interface() or donor:name() ~= faction_key or patron:is_null_interface() or patron:is_dead()
        or not patron:has_capital_region() or patron:is_rebel() or patron:is_world_leader()
        or patron:name() == "__HAN_EMPIRE__" then return end
    return patron
end

local function find_new_patron(owner)
    local deals = owner:diplomatic_deal_list()
    local latest, patron = -1, nil
    for i = 0, deals:num_items() - 1 do
        local deal = deals:item_at(i)
        local id = deal:cqi()
        local seen = patron_value .. "_deal_" .. id
        if not cm:get_saved_value(seen) then
            local candidate = sponsored_warlord(deal)
            cm:set_saved_value(seen, true)
            if candidate and id > latest then
                latest, patron = id, candidate
            end
        end
    end
    if not patron or patron:name() == cm:get_saved_value(patron_value) then return end

    cm:set_saved_value(patron_value, patron:name())
    local seat = patron:capital_region():settlement()
    cm:modify_faction(owner):show_message_event_located(
        "event_feed_strings_text_" .. faction_key .. "_patron_title",
        "event_feed_strings_text_" .. faction_key .. "_patron_detail",
        "event_feed_strings_text_" .. faction_key .. "_patron_body",
        seat:logical_position_x(), seat:logical_position_y(), true, 0)
end

local function record_sack(region)
    if not region:is_province_capital() then return end
    local already_sacked = sack_count .. "_" .. region:name()
    local total = cm:get_saved_value(sack_count) or 0
    if total >= sack_target or cm:get_saved_value(already_sacked) then return end
    total = total + 1
    cm:set_saved_value(already_sacked, true)
    cm:set_saved_value(sack_count, total)
    core:trigger_event(faction_key .. "_raid_completed")
    local owner = cm:query_faction(faction_key)
    local loot = 0
    if owner:has_technology(faction_key .. "_ransom_ledgers") then loot = loot + 500 end
    if owner:has_technology(faction_key .. "_rolling_plunder") then loot = loot + 750 end
    local deadline = cm:get_saved_value(contract_until) or 0
    if deadline >= cm:query_model():turn_number() then
        loot = loot + 2500
        cm:set_saved_value(contract_until, 0)
        cm:remove_effect_bundle(heat_key, faction_key)
    end
    if loot > 0 then cm:modify_faction(owner):increase_treasury(loot) end
    if total == sack_target then
        award_victory()
    else
        local site = region:settlement()
        local loc = "event_feed_strings_text_" .. faction_key .. "_sack_"
        cm:modify_faction(faction_key):show_message_event_located(
            loc .. "title", loc .. total, "", site:logical_position_x(), site:logical_position_y(), true, 0)
    end
end

cm:add_first_tick_callback(function()
    if not player_faction() or cm:get_saved_value(victory_value) then return end
    if is_cult then
        if (cm:get_saved_value(sack_count) or 0) >= sack_target then award_victory() end
    else
        local patron_key = cm:get_saved_value(patron_value)
        if patron_key then
            local patron = cm:query_faction(patron_key)
            if patron and not patron:is_null_interface() then check_patron(patron) end
        end
    end
end)

if is_cult then
    core:add_listener(faction_key .. "_contract_offer", "FactionTurnStart",
        function(context)
            return context:faction():name() == faction_key and player_faction()
                and not cm:get_saved_value(victory_value)
        end,
        function(context)
            local turn = cm:query_model():turn_number()
            local deadline = cm:get_saved_value(contract_until) or 0
            if deadline > 0 and turn > deadline then
                cm:set_saved_value(contract_until, 0)
                core:trigger_event(faction_key .. "_contract_expired")
                if context:faction():has_effect_bundle(heat_key) then
                    cm:remove_effect_bundle(heat_key, faction_key)
                end
            end
            if (cm:get_saved_value(sack_count) or 0) > 0
                and context:faction():has_technology(faction_key .. "_fences")
                and turn >= (cm:get_saved_value(next_contract) or 0) then
                cm:set_saved_value(next_contract, turn + 8)
                cm:trigger_dilemma(faction_key, contract_key, true)
            end
        end, true)

    core:add_listener(faction_key .. "_contract_choice", "DilemmaChoiceMadeEvent",
        function(context)
            return context:faction():name() == faction_key and context:dilemma() == contract_key
                and context:choice() == 0
        end,
        function()
            cm:set_saved_value(contract_until, cm:query_model():turn_number() + 4)
            cm:apply_effect_bundle(heat_key, faction_key, -1)
        end, true)

    core:add_listener(faction_key .. "_sack", "CharacterPerformsSettlementSiegeAction",
        function(context)
            return context:query_character():faction():name() == faction_key and player_faction()
                and string.find(context:action_option_record_key(), "sack_and_withdraw", 1, true)
        end,
        function(context)
            record_sack(context:garrison_residence():region())
        end, true)
else
    core:add_listener(faction_key .. "_gift", "DiplomacyNegotiationFinished",
        function() return player_faction() and not cm:get_saved_value(victory_value) end,
        function() find_new_patron(cm:query_faction(faction_key)) end, true)

    core:add_listener(faction_key .. "_gift_turn_end", "FactionAboutToEndTurn",
        function(context)
            return context:faction():name() == faction_key and player_faction() and not cm:get_saved_value(victory_value)
        end,
        function(context) find_new_patron(context:faction()) end, true)

    for _, event in ipairs({"FactionBecomesWorldLeader", "FactionBecomesWorldLeaderCaptureSettlement", "FactionTurnStart"}) do
        core:add_listener(faction_key .. "_" .. event, event,
            function(context) return context:faction():name() == cm:get_saved_value(patron_value) end,
            function(context) check_patron(context:faction()) end, true)
    end
end
