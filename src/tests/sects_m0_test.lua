-- Run with RPFM's Lua test harness (needs RPFM's Lua API data from the Assembly Kit).
-- Kept outside script/ so the game never loads it.

local zhang = rpfm.faction { key = "3k_main_faction_zhang_yan", is_human = true }

rpfm.test("sect script loads and exposes its table", function()
    rpfm.assert_equal(type(SECTS), "table", "SECTS global")
end)

rpfm.test("startup first tick applied the M0 bundle", function()
    rpfm.assert_called("apply_effect_bundle", "sects_m0_bundle", "3k_main_faction_zhang_yan", 3)
end)

rpfm.test("a turn does not break the script", function()
    rpfm.end_turn()
    rpfm.assert_equal(type(SECTS), "table")
end)
