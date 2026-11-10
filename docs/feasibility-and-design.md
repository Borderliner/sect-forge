# Sects & Cults — Feasibility & Technical Design

Total War: Three Kingdoms. Target: campaign 190 (`3k_main`) first, DLC maps later.
Tooling: Assembly Kit + RPFM (`C:\Users\Touraj\Apps\RPFM`) + Script Extender 0.46.0.

Everything below was verified against the local install, not recalled from memory. Paths are
included so each claim can be re-checked.

---

## 1. Verdict

| Feature | Possible? | How |
|---|---|---|
| Custom sects / cults as playable or AI groups | **Yes** | New `factions` + `subcultures` rows, or reuse `3k_dlc05_subculture_bandits` |
| A sect "tree" (progression) | **Yes** | Custom technology tree: `technologies`, `technology_nodes`, `technology_node_sets`, `technology_categories` — vanilla already ships faction-specific trees |
| Beliefs / doctrines that change playstyle | **Yes** | `effect_bundles` + `effects`, plus **runtime-defined** bundles via Script Extender |
| Hideouts that exist on the map | **Partial** | Only in *existing* building slots (resource slots). Free placement needs campaign-map editing |
| Hideout "hidden for 3 turns" | **Yes** | Lua state + ambush stance + effect bundle; SE slot natives for construction |
| Bandit / preacher / assassin / mercenary roles | **Yes** | Spy actions already exist in data (`agent_actions`), plus traits, CEOs, effects |
| Characters affected by beliefs | **Yes** | `character_traits`, `character_skills`, `character_skill_level_to_effects_junctions`, CEOs |
| Brand-new custom UI panel | **Hard** | Game UI is TWUI (Animate/Flash-style components). Possible, but the largest and riskiest chunk |
| Save-game compatibility | **Fragile** | Any DB/script change after a campaign starts can break saves; plan "new campaign per major version" |

Bottom line: everything you described is expressible in 3K's data-driven framework except
"put a hideout at an arbitrary point on the map". That one needs one of: (a) resource slots,
(b) regionless/horde-style forces, or (c) real campaign-map editing in DaVE.

---

## 2. What the game already gives us

Assembly Kit raw DB: `...\Total War THREE KINGDOMS\assembly_kit\raw_data\db\` — 1,259 XML-exported tables.

### 2.1 There is already a bandit / cult lineage

`factions.xml` → `3k_main_faction_zhang_yan`:

```
<subculture>3k_dlc05_subculture_bandits</subculture>
<feature_forest>3k_main_faction_feature_forest_locked_outlaw</feature_forest>
<military_group>3k_dlc05_military_grouping_bandits</military_group>
```

`technology_node_sets.xml` ships **five** separate bandit-network trees per element:

```
3k_dlc05_tech_set_bandit_network_cults_{earth,fire,metal,water,wood}
3k_dlc05_tech_set_bandit_network_bandits_*
3k_dlc05_tech_set_bandit_network_crooks_*
3k_dlc05_tech_set_bandit_network_tribes_*
```

So "a faction with its own tree" is not a hack — it is exactly what the reform (tech) UI
already does for Yellow Turbans, Nanman and the bandit network.

### 2.2 Bandit buildings already act as hideouts

- `slot_templates.xml` → `3k_resource_bandit` (a slot template used in resource slots).
- `building_chains.xml` → `3k_resource_bandit_black_market`, `_charity`, `_espionage`,
  `_gang_recruitment`, `_white_tiger_yan_shanyue`, superchain `3k_resource_bandit_buildings`.
- `3k_district_residential_bandits`, `3k_district_government_black_mountain`.

These are the closest analogue to a sect hideout: faction-specific, upgradeable, built into
existing map slots.

### 2.3 Positions are *not* in the DB

- `campaign_map_settlements.xml`: only `settlement_id`, `climate_type`, `citybar_height_offset`.
- `campaign_map_regions.xml`: only `campaign_map` + `region`.

Map geometry and building-slot layout live in campaign-map data authored in DaVE
(`assembly_kit\dave\DaVE.retail.x64.exe`, `binaries\bob*.exe`). The DB cannot move or create a
slot.

**Exception:** Areas of interest *do* have coordinates in the DB —
`campaign_map_areas_of_interest_label_positions.xml` has `logical_position_x` / `logical_position_y`
(e.g. `3k_dlc06_main_aoi_mount_heng_north` at 421, 275). Reusing AOIs as "hideout sites" is a
viable shortcut; *adding* new AOIs still goes through the map editor.

### 2.4 Regionless / horde support exists

- `faction_features.xml`: `CAN_BE_REGIONLESS`, `CAN_RECRUIT_HORDES`, `CAN_SUPPORT_HORDES`,
  `CAN_OCCUPY_SETTLEMENTS`.
- `slot_templates.xml`: `horde_primary`, `horde_secondary`.
- `start_pos_horde_details.xml` exists.
- Script Extender has a dedicated `se_horde_income_hook` and a `horde_income` config flag.

This is the path to "a hideout that can sit anywhere": a force-carried settlement rather than a
map slot. Vanilla 3K barely uses it, so it needs a spike before committing.

### 2.5 Cover / concealment primitives

`campaign_stances.xml` includes `AMBUSH`, `SET_CAMP`, `SET_CAMP_RAIDING`, `LAND_RAID`,
`SEA_RAID`, `TUNNELING`, `PATROL`, with `campaign_stances_factions_junctions.xml` to restrict
stances per faction.

### 2.6 Spy actions already cover assassin / preacher / agent work

`agent_actions.xml` ships `3k_main_spy_action_*` including:

- `character_assassinate_faction_leader`, `character_assassinate_faction_heir`
- `region_open_gates`, `region_weaken_garrison`, `region_bolster_garrison`
- `region_spy_network`, `region_surrender_province`
- `lower_public_order`, `trigger_yellow_turban_rebellion`
- `faction_steal_item`, `faction_improve_relations`, `accost_traders`

Agent types in `agents.xml` are only `general`, `colonel`, `castellan` — spies are character
subtypes, not a separate agent list.

### 2.7 Effects / bundles / religion

- `effects.xml`: 1,935 rows.
- `effect_bundles.xml`, `effect_bundles_to_effects_junctions.xml`, `effect_bundle_targets.xml`.
- `religions.xml`: `3k_main_buddhist`, `3k_main_confucianism`, `3k_main_taoist`,
  `3k_main_yellow_turban`, plus `region_religions.xml` + `religion_conversion_mods.xml`.

Belief spread can ride the existing religion system instead of being invented.

### 2.8 UI is TWUI

`assembly_kit\binaries\uicomponentlib.modder.x64.dll` contains `TWUI::ComponentContextReturnValue`
and related symbols. So a custom sect panel is an Animate/TWUI authoring job, not a Lua-only job.

---

## 3. Script Extender 0.46.0 (installed)

`%APPDATA%\TKModManager\dll\0.46.0\script_extender.dll` (pinned via `pinned.txt`, config in
`script_extender.cfg`). `se_inventory.json` lists 89 natives. Relevant ones:

**Buildings / slots**

```
se_slot_info  se_slot_candidates  se_slot_items
se_slot_construct  se_slot_pay_to_complete  se_slot_damage  se_slot_repair  se_slot_destroy
```

Signature parsed from the Lua embedded in the DLL:

```lua
se_slot_construct(slot, faction, level_key, force, ?, free, turns)
se_slot_candidates(slot, only_valid, all_chains)
```

Error strings confirm it handles locked slots, in-progress construction, and level ranges.

**Runtime effect bundles (the "beliefs" engine)**

```
se_effect_bundle_define  se_effect_bundle_apply_custom  se_effect_bundle_restore  se_effect_bundle_info
se_faction_effect_value  se_faction_gdp_bonus
```

**Characters / CEOs**

```
se_char_info  se_char_xp_add  se_char_rank_get  se_skill_points_set
se_ceo_refresh_info  se_ceo_rebuilt  se_ceo_refresh_selftest
```

**Forces / economy**

```
se_recruit_unit  se_unit_info  se_unit_strength_get/set  se_unit_xp_get/set
se_world_leaders  se_world_leader_force  se_horde_income_hook
```

**Presentation / misc**

```
se_alliance_name_set  se_dilemma_choice  se_save_chunking  se_log  se_status  se_version
```

The DLL also embeds a Lua helper library exposing `se.available(name)`, `se.version()`,
`se.status()`, `se.character(cqi)`, `se.faction(key)`, `se.query`, `se.modify`, `se.on_model`.
Depend on `se.available(...)` checks so the mod still loads when SE is missing.

---

## 4. Recommended architecture

**Sect = a data-defined outlaw faction lineage; beliefs = effect bundles; hideouts = bandit
resource-slot buildings with a Lua concealment state.**

```
Sect faction (factions/subcultures)
 ├─ Sect tree   → technology_node_sets + technology_nodes + technology_categories
 ├─ Beliefs     → effect_bundles (+ SE runtime bundles for situational ones)
 ├─ Hideout     → building chain in 3k_resource_bandit slots
 ├─ Concealment → Lua state + AMBUSH/SET_CAMP stance + "concealed" bundle
 ├─ Roles       → spy character subtype + reskinned spy actions
 └─ Exposure    → 3-turn timer in Lua, saved with cm:save_named_value
```

### Option A vs B — decision

**Chosen: A — one faction per sect.** Each sect gets its own `factions`/`subcultures` rows,
flag, colours, startpos placement, AI personality and tree. Highest cost, but no runtime
renaming limits and the cleanest flavour.

Consequence: build a reusable generator early (for faction rows, tree, building chain and loc)
so adding the 4th/10th sect is cheap. A generic "Sect" faction is still useful as a test bed.

### Status

M0 is built and validated (see `README.md`): effect bundle, bandit-slot hideout chain with three
levels, loc, and a Lua smoke script. Pack: `build/SectsCults_M0.pack`.

### Hideout lifecycle (v1)

1. Player selects a resource slot owned by/adjacent to a sect-eligible region.
2. `se_slot_construct(slot, faction, "sect_hideout_1", false, false, false, turns)` plants the
   hideout; upgrade levels are a normal `building_chains` chain.
3. On construction complete: set `hidden_until = turn + 3`, apply concealment bundle, switch the
   force to `AMBUSH`/`SET_CAMP`.
4. Each turn tick: if `turn > hidden_until` or an enemy character is in an adjacent region with
   detection, reveal (remove bundle, log an incident/dilemma).
5. `se_slot_destroy` on abandonment; state in `cm:save_named_value`.

### Sect tree (v1)

Five elements × 3 tiers = 15 nodes, keyed `sect_tech_<element>_tier<n>`, wired into a new
`technology_category` with `technology_category_ui_colours` so the UI reads correctly. Each node
has `technology_effects_junction` rows only for passive bonuses; active beliefs go through
bundles so they can be swapped.

### Beliefs (v1)

| Belief | Type | Implementation |
|---|---|---|
| Asceticism | passive | `effect_bundles` upkeep/income modifiers |
| Militancy | passive+active | recruit discount + `se_recruit_unit` pool unlock |
| Secrecy | situational | runtime bundle: +detection resistance while hidden |
| Proselytism | spread | `religion_conversion_mods` + region religion effect |
| Patronage | character | trait/CEO grant via `se_char_xp_add`, `se_ceo_*` |

---

## 5. Milestones

**M0 — smoke test (1 pack, no gameplay).**
Pack loads with RPFM; one custom effect bundle shows in-game; one custom building chain appears in
a bandit resource slot; Lua prints a line via `se_log`/`out`.

**M1 — hideout.**
Build / upgrade / abandon a hideout; 3-turn concealment; reveal on timeout or detection.

**M2 — sect tree + beliefs.**
Custom reform tree renders for the sect faction; three beliefs apply/remove bundles.

**M3 — roles.**
Two spy actions reskinned for assassins; mercenary unit recruitment from hideouts.

**M4 — polish.**
Loc, icons, AI weighting, DLC maps, optional TWUI panel.

---

## 6. Risks

- **Script Extender dependency.** Pin 0.46.0 and guard every native with `se.available()`.
  SE also enforces a multiplayer version lock — co-op players must match.
- **Map mods.** Your current load order includes region-expanding overhauls
  (`!!190_expanded_region`, `!!190_expanded_with_characters`). Slot-based hideouts depend on the
  map, so those mods will change which slots exist. Test with and without.
- **Custom map work is a different project.** Adding slots/AOIs means DaVE + regenerated
  startpos, which breaks compatibility with every other map mod.
- **Saves.** Schema changes mid-campaign break saves; version the mod and document it.
- **UI.** A custom panel is TWUI authoring. Do not block v1 on it — reuse the reform tree,
  building browser and dilemma/incident dialogs.

---

## 7. Files this mod will create

```
data/db/factions/*.tsv (or xml via RPFM)
data/db/subcultures/*.tsv
data/db/building_chains/*.tsv
data/db/building_levels/*.tsv
data/db/building_chain_availabilities/*.tsv
data/db/effect_bundles/*.tsv
data/db/effect_bundles_to_effects_junctions/*.tsv
data/db/technologies/*.tsv
data/db/technology_nodes/*.tsv
data/db/technology_node_sets/*.tsv
data/db/technology_categories/*.tsv
data/db/agent_subtypes/*.tsv
data/db/character_traits/*.tsv
data/text/db/*.loc
script/campaign/mod/sect_main.lua
script/campaign/mod/sect_hideout.lua
script/campaign/mod/sect_beliefs.lua
```

Source of truth for table schemas: `assembly_kit\raw_data\db\<table>.xml` and the matching
`.xsd` files next to them.
