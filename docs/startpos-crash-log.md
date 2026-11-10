# Startpos crash investigation (3k_main, 2026-11-06)

Goal: add a brand-new faction (`sects_white_lotus`) to the 190 campaign without replacing any
existing faction, then give it an officer and a starting city.

## What is proven

| Test | Result |
| --- | --- |
| Vanilla AK data | builds |
| New faction row in `factions` + `start_pos_factions` (no region, no character) | builds |
| New faction row only, `playable=1` | builds |
| Extra on-map character cloned into **Zhai Rong's own faction** | builds |
| Extra pool character cloned into **Zhai Rong's own faction** | builds |
| Dormant vanilla faction (Wuwei) given Hepu + an on-map officer | builds |
| New faction + officer | **crash 0xc0000005 @ 0x1a3c896** |
| New faction + Hepu ownership + officer | **crash** |
| New faction + officer, `playable=0`, generic/donor CEO | **crash** |
| ... + all faction link rows cloned from donor (banners, name group, uniforms, stances, agents, groups) | **crash** |
| ... + unique=0 custom template | **crash** |
| ... + separatist twin + civil-war setup | crash (twin without officer still untested in isolation) |
| ... + officer using a *vanilla, non-unique, startpos-used* template (`3k_dlc07_template_generated_chen_bai_hero_earth`) | see latest build |

## Localisation (the breakthrough)

The game resolves a faction's display name through `factions_screen_name_<faction_key>`,
`factions_screen_adjective_<key>` and `factions_screen_name_when_rebels_<key>`, and character
names through `names_name_<id>`.

- `factions_screen_name_3k_main_faction_wuwei` exists in vanilla `text/db/factions__.loc`.
- `factions_screen_name_sects_white_lotus` existed **nowhere** in the build.
- The AK has no `raw_data/text` folder, so Bob cannot build localisation for new keys.
- A faction with no region and no character is never named, which is why those builds passed.

`tools/MakeLocPack.ps1` builds a loc-only pack with those keys and installs it into the game's
`data/` folder as a **Movie** pack (`data/sects_white_lotus_loc.pack`), which the game loads even
during startpos generation. The same loc rows must ship inside the final mod pack.

## Narrowed to one thing

| Test | Result |
| --- | --- |
| Dormant vanilla faction (Wuwei) + Hepu + officer with a **vanilla** template | builds |
| Dormant vanilla faction (Wuwei) + Hepu + officer with **our custom template and custom name ids** | builds |
| New faction row (any variant) + Hepu + officer | crash |

Everything about the officer is fine: custom template (`unique=0`, cloned from
`3k_dlc05_template_generic_bandit_wood_governor_normal_m_hero`), custom name ids, custom art set,
retinue, on-map placement. The failure appears only when a **brand-new faction row** is appended to
`start_pos_factions` (new key + new numeric id).

## Root cause (probable) and fix

Bob's startpos step **launches the game**, and that game instance only has the vanilla database
unless our DB tables are present as a loaded pack. Consequences:

- A `start_pos_factions` row for an unknown faction key cannot be resolved -> crash.
- A row for a *vanilla* faction that is absent from the 190 campaign (`3k_dlc04_faction_bian_zhang`)
  builds fine, which proves the key lookup is the failing step.

Fix: `tools/MakeSupportPack.ps1` copies every DB table our faction touches (plus its loc) out of the
Assembly Kit into a pack, saves it as a **Movie** pack and installs it to
`data/sects_white_lotus_support.pack`, so the game always loads the new faction during the build.
The same pack is the data part of the final mod.

## The two real blockers

1. **A brand-new faction key cannot be resolved while the game builds the startpos.** Bob launches the
   game; that instance reads the campaign pack's `db/start_pos_*` tables but resolves faction keys
   against the retail database. A key that only exists in the Assembly Kit is unresolved -> crash.
   A vanilla faction that is simply absent from the 190 campaign (`3k_dlc04_faction_bian_zhang`)
   builds fine, which isolates this exactly.
2. **`start_pos_factions.playable = 1` needs the campaign-selection chain.** All 24 playable 190
   factions have a `frontend_factions` row; none of the non-playable ones do. A playable faction
   without `frontend_factions` -> `frontend_faction_to_frontend_faction_leaders` ->
   `frontend_faction_leaders` -> `frontend_characters` crashes the build.

## Chosen design

Use a faction that already exists in the database **and is dormant in every campaign**: Wuwei
(`3k_main_faction_wuwei`; 0 regions, 0 characters in all four campaigns it appears in). Nothing is
replaced anywhere - the slot was never used. The sect then gets:

- its own display name, adjective, colours (DB overrides + loc),
- its own officer (custom non-unique template + custom name ids + art set),
- a starting city,
- the standard frontend campaign-selection chain so it is selectable and playable.

Tools: `tools/AddFrontendData.ps1` builds the frontend chain; `tools/StartposGen.ps1 -SkipFactionRow`
reuses the slot's existing startpos row instead of adding one.

## Ruled out

- Missing DB link rows: cloned all 10 tables that reference a faction key
  (`building_chain_availabilities`, `cai_personality_group_overrides`,
  `campaign_group_member_criteria_factions`, `campaign_map_attrition_faction_immunities`,
  `campaign_stances_factions_junctions`, `faction_banners`, `faction_agent_permitted_subtypes`,
  `faction_to_name_groups`, `faction_uniform_colours`, `unit_banners`).
- `factions.index` value (tested donor-adjacent value).
- `factions.category` / `start_pos_factions.playable`.
- `start_pos_factions.ceo_initial_data` (generic vs donor-specific).
- Character template uniqueness (a unique template reused by a second character builds fine).
- Duplicate/short startpos IDs, ID collisions, sort order, UTF-8/BOM issues (startpos XML is ASCII).
- Region ownership alone (Hepu was never tested alone with complete faction data — pending).
- Faction groups keyed by ID, political parties, victory conditions, technologies,
  world power tokens, family relationships.

## Key mechanics learned

- The AK startpos XMLs are UTF-8 **without BOM**; read/write them with explicit UTF-8 and no BOM
  (`tools/StartposGen.ps1` now does this).
- 62 factions own regions and *all* of them have a `faction_leader` character; 208 factions have
  neither regions nor characters (dormant).
- Every "real" faction has a `_separatists` twin row in `factions` and a `faction_civil_war_setups`
  row linking the pair; 318 factions have one.
- Dormant factions can be activated (Wuwei + Hepu + officer builds), so the engine does not require
  a faction to have existed in the vanilla startpos.
- A faction with a *scripted* character template retinue is a strong crash suspect: our first custom
  template was cloned from `3k_dlc04_template_scripted_han_chinese_wood_average_early`.
  The template donor is now chosen among `unique=0` templates that are **already used in
  `start_pos_characters`**, with a matching subtype.

## Tooling written for this

- `tools/BobBuild.ps1` - opens Bob, clicks Select all + Start through UI Automation, waits, reports.
- `tools/StartposGen.ps1` - writes faction / leader / retinue / region / settlement rows
  (switches: `-SkipLeader`, `-SkipRetinue`, `-SkipRegion`, `-SkipSettlement`, `-Playable`,
  `-CeoInitialData`, `-LeaderTemplate`).
- `tools/ActivateFactionTest.ps1` - control: give a dormant vanilla faction a city and an officer.
- `tools/CtrlCharTest.ps1` - control: add a cloned officer to an existing faction (pool/map/leader).
- `tools/CloneFactionLinks.ps1` - clones every DB row that references a donor faction key.
- `tools/AddSeparatorTwin.ps1` - creates `<faction>_separatists`, its link rows, the civil-war
  setup and the separatist `start_pos_factions` row.
