# Sects & Cults — Total War: Three Kingdoms mod

## Sect Forge

Open `SectForge.bat`. Enter your faction and leader names, choose two colours, and select an existing portrait. **Browse mod…** opens the compatible portraits from an installed `.pack`. A pack built with a mod portrait depends on that source pack, which must stay available to the game.

**Build mod pack** exports `sects_<name>.pack` to `%LOCALAPPDATA%\SectForge\mods`. Click **Open output folder**, import the pack into your mod manager, and enable it before starting a new 190 CE campaign in Romance or Records. Use the exported folder or your mod manager's library when loading the pack. Launching directly with a working directory under `Sects&Cults` caused a startup crash during testing.

This scenario adds a distinct faction and custom leader at Hepu. It uses a male Champion (Wood) leader and supports one generated faction pack at a time. The scenario supplies the flag artwork and campaign selection blurb. Campaign overhauls that supply their own 190 start positions conflict with the generated scenario. Close the game before rebuilding an existing pack.

The editor requires Windows, Python 3.10 or newer with Tkinter, and an RPFM Server with Three Kingdoms configured. It looks for `rpfm_server.exe` on `PATH` and in `%USERPROFILE%\Apps\RPFM`. Set `SECTFORGE_RPFM` to use a different executable. The builder uses `build/SectsCults_white_lotus.pack` as its input scenario.

For command-line builds:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\BuildSect.ps1 -ConfigPath configs\white_lotus.json
```

The CLI also accepts `portraitPack` with a path to the source `.pack`, and `artSet` with the corresponding art-set key. The Hepu scenario supplies the start position, IDs and gameplay template. Existing config fields for start coordinates, city, numeric IDs and voice have no effect on this build path.

## Original Sects & Cults prototype

Custom sects/cults that live in hideouts, follow their own belief trees and act as bandits,
preachers, assassins or mercenaries.

- **Feasibility + architecture:** [`docs/feasibility-and-design.md`](docs/feasibility-and-design.md)
- **Status:** M0 (smoke test) builds and validates.

## Layout

```
docs/feasibility-and-design.md   what the game supports, why, and the plan
src/script/sects_m0.lua          campaign Lua (source of truth)
src/tests/sects_m0_test.lua      RPFM Lua test (kept outside script/ so the game ignores it)
src/tsv/*.tsv                    the pack's tables exported as RPFM TSVs (reference)
tools/rpfm_mcp.ps1               minimal MCP client for rpfm_server
tools/build_m0.ps1               rebuilds build/SectsCults_M0.pack from scratch
build/SectsCults_M0.pack         the built mod
```

## Prerequisites

- Total War: Three Kingdoms + Assembly Kit (installed)
- RPFM 5.x at `C:\Users\Touraj\Apps\RPFM` (3K schema downloaded)
- Script Extender 0.46.0 registered through TK Mod Manager (game-side testing)
- PowerShell 5.1

## Build

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\build_m0.ps1
```

The script starts `rpfm_server.exe` if needed, talks to it over MCP
(`http://127.0.0.1:45127/mcp`), creates the pack, adds rows, saves and runs diagnostics.

Expected diagnostics output once the Assembly Kit-backed dependency cache exists:

```
diagnostics: {"by_level":{"info":2},"by_type":{"NoReferenceTableNorColumnFoundPak":2},"total":2}
```

The two `info` entries are `commodity` and `resource_requirement` on `building_levels`: RPFM's
schema has no reference column for them. They are tool schema gaps, not data errors.
**There are no warnings and no errors.**

If the cache has not been regenerated since installing the Assembly Kit, the same run shows four
`NoPak` warnings for references into AK tables (`commodity`, `resource_requirement`,
`bundle_target`, `icon_supplement_colour_type`). Run `Game Selected -> Generate Dependencies
Cache` once in RPFM to clear them; that is what this project did.

## What M0 contains

| Pack path | Rows |
|---|---|
| `db/effect_bundles_tables/sects_cults` | `sects_m0_bundle` (faction target, positive) |
| `db/effect_bundles_to_effects_junctions_tables/sects_cults` | bundle -> background income effect |
| `db/building_chains_tables/sects_cults` | `sects_hideout` in the bandit superchain |
| `db/building_levels_tables/sects_cults` | `sects_hideout_1..3` (copied from a vanilla bandit level) |
| `db/building_chain_availability_sets_tables/sects_cults` | `sects_hideout` in `3k_main`, `3k_dlc06_nanman`, `ep_default`, `3k_dlc05_bandit`, `3k_dlc05_bandit_white_tiger_yan` |
| `db/building_set_to_building_junctions_tables/sects_cults` | `sects_hideout` in `3k_main_building_set_all` and `3k_dlc05_bandit_resource_buildings` |
| `text/db/sects_cults.loc` | names/descriptions for the bundle and the chain |
| `script/campaign/mod/sects_m0.lua` | logs SE status and applies the bundle for 3 turns |

The chain is hooked into `3k_resource_bandit_buildings`, the superchain mapped to the
`3k_resource_bandit` slot template, which is the same route vanilla bandit buildings use.

## In-game test (M0)

1. Copy `build\SectsCults_M0.pack` into
   `C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\data\`
   (or your TKMM mods folder) and enable it.
2. Start a **new** campaign (any faction; the hideout is now in the wide availability sets).
3. Open a **resource (secondary) slot**, not the province capital's main slot. `3k_resource_bandit`
   is the slot template used by roughly 600 resource slots across the map. **Sect Hideout** should
   be listed at 500 gold.
4. Check `lua_mod_log.txt` in the game root for:

   ```
   [sects] file executing
   [sects] first-tick callback registered=true
   [sects] M0 loaded - state=ready version=...
   [sects] applied sects_m0_bundle for 3 turns
   ```

If the building does not appear, check, in order: the availability set (`3k_dlc05_bandit`), the
superchain (`3k_resource_bandit_buildings`), and whether that faction's subculture can own the
slot type at all.

## Editing the data

- **Quick iteration:** open `build\SectsCults_M0.pack` in RPFM, edit the tables in the grid, save.
- **Reproducible:** change the values in `tools/build_m0.ps1` (source of truth) and rebuild.
  `src/tsv/*.tsv` are exports for reference and spreadsheet work; the build script does not read
  them.

## Next milestone (M1)

Hideout lifecycle: 3-turn concealment, reveal on timeout or detection, `se_slot_construct` /
`se_slot_destroy` so Lua can plant and remove hideouts, and state persisted with
`cm:save_named_value`. See the design doc, section 4.
