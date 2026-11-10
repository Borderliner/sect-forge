# Builds the M0 smoke-test pack for Sects & Cults through the RPFM server (MCP).
# Requires rpfm_server.exe; the script starts it if it is not running.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\build_m0.ps1
#
# Output: build\SectsCults_M0.pack

. "$PSScriptRoot\rpfm_mcp.ps1"

$ErrorActionPreference = 'Stop'
Start-Rpfm
Connect-Mcp

$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

# Close any earlier build so repeated runs stay clean.
$status = (Mcp-Tool 'session_status' @{}).structuredContent
foreach ($p in $status.packs) {
    if ($p.name -eq 'SectsCults_M0.pack') { $null = Mcp-Tool 'close_pack' @{ pack = $p.key } }
}

$packInfo = (Mcp-Tool 'new_pack' @{ name = 'SectsCults_M0.pack' }).structuredContent
$pack = $packInfo.key
"pack key: $pack"

function New-DbTable([string]$path) {
    $tableName = ($path -split '/')[1]
    $null = Mcp-Tool 'create_file' @{ pack = $pack; path = $path; kind = @{ type = 'db'; table_name = $tableName } }
    "created $path"
}

function Add-Rows([string]$path, $edits) {
    $r = Mcp-Tool 'edit_table' @{ pack = $pack; path = $path; edits = $edits }
    if ($r.isError) { throw ("edit_table failed for $path : " + ($r.content.text -join ' ')) }
    "rows added to $path"
}

# --- tables -----------------------------------------------------------------
New-DbTable 'db/effect_bundles_tables/sects_cults'
New-DbTable 'db/effect_bundles_to_effects_junctions_tables/sects_cults'
New-DbTable 'db/building_chains_tables/sects_cults'
New-DbTable 'db/building_levels_tables/sects_cults'
New-DbTable 'db/building_chain_availability_sets_tables/sects_cults'
New-DbTable 'db/building_set_to_building_junctions_tables/sects_cults'

# --- the test belief ---------------------------------------------------------
Add-Rows 'db/effect_bundles_tables/sects_cults' @(
    @{ op = 'insert'; values = @{
            key                            = 'sects_m0_bundle'
            localised_description          = 'A test belief used to prove the sect pipeline works.'
            localised_title                = 'Sect Belief: Asceticism'
            ui_icon                        = '.'
            bundle_target                  = 'faction'
            icon_supplement_colour_type    = 'positive'
            show_in_generic_bundle_display = $true
        } }
)

Add-Rows 'db/effect_bundles_to_effects_junctions_tables/sects_cults' @(
    @{ op = 'insert'; values = @{
            effect_bundle_key = 'sects_m0_bundle'
            effect_key        = '3k_main_effect_economy_character_wealth_faction_background_income_mod'
            effect_scope      = 'faction_to_faction_own_unseen'
            value             = 1
            advancement_stage = 'start_turn_completed'
        } }
)

# --- the hideout building chain ---------------------------------------------
Add-Rows 'db/building_chains_tables/sects_cults' @(
    @{ op = 'insert'; values = @{
            key                  = 'sects_hideout'
            tech_category_tab    = ''
            tech_category_position = ''
            chain_category       = 'money'
            in_encyclopedia      = $true
            building_superchain  = '3k_resource_bandit_buildings'
            optional_tier_icon   = ''
            optional_sort_order  = 0
        } }
)

$vanillaLevel = @{ file = @{ source = 'game_files'; path = 'db/building_levels_tables/data__' }; index = 275 }
$levelEdits = @()
$levels = @(
    @{ key = 'sects_hideout_1'; level = 0; cost = 500; time = 1; upkeep = 0 },
    @{ key = 'sects_hideout_2'; level = 1; cost = 1000; time = 2; upkeep = 50 },
    @{ key = 'sects_hideout_3'; level = 2; cost = 2000; time = 2; upkeep = 100 }
)
foreach ($l in $levels) {
    $levelEdits += @{ op = 'insert'; copy_from = $vanillaLevel; values = @{
            level_name                  = $l.key
            chain                       = 'sects_hideout'
            level                       = $l.level
            create_cost                 = $l.cost
            cost_to_construct_instantly = $l.cost
            create_time                 = $l.time
            upkeep_cost                 = $l.upkeep
        } }
}
Add-Rows 'db/building_levels_tables/sects_cults' $levelEdits

Add-Rows 'db/building_chain_availability_sets_tables/sects_cults' @(
    @{ op = 'insert'; values = @{ building_chain = 'sects_hideout'; id = '3k_main' } },
    @{ op = 'insert'; values = @{ building_chain = 'sects_hideout'; id = '3k_dlc06_nanman' } },
    @{ op = 'insert'; values = @{ building_chain = 'sects_hideout'; id = 'ep_default' } },
    @{ op = 'insert'; values = @{ building_chain = 'sects_hideout'; id = '3k_dlc05_bandit' } },
    @{ op = 'insert'; values = @{ building_chain = 'sects_hideout'; id = '3k_dlc05_bandit_white_tiger_yan' } }
)

# Vanilla chains are registered in the global building set; bandit chains also in the
# bandit resource set. Missing these was why the hideout never appeared.
Add-Rows 'db/building_set_to_building_junctions_tables/sects_cults' @(
    @{ op = 'insert'; values = @{ building_chain = 'sects_hideout'; building_level = ''; building_set = '3k_main_building_set_all'; exclude = $false } },
    @{ op = 'insert'; values = @{ building_chain = 'sects_hideout'; building_level = ''; building_set = '3k_dlc05_bandit_resource_buildings'; exclude = $false } }
)

# --- localisation ------------------------------------------------------------
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = 'text/db/sects_cults.loc'; kind = @{ type = 'loc' } }
Add-Rows 'text/db/sects_cults.loc' @(
    @{ op = 'insert'; values = @{ key = 'effect_bundles_localised_title_sects_m0_bundle'; text = 'Sect Belief: Asceticism'; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = 'effect_bundles_localised_description_sects_m0_bundle'; text = 'A test belief used to prove the sect pipeline works.'; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = 'building_chains_onscreen_name_sects_hideout'; text = 'Sect Hideout'; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = 'building_chains_onscreen_description_sects_hideout'; text = 'A concealed place where a sect gathers, trains and hides.'; tooltip = $false } }
)

# --- script ------------------------------------------------------------------
# write_file with "text" contents stalls on this server build; raw bytes are instant.
$luaPath = Join-Path $PSScriptRoot '..\src\script\sects_m0.lua'
$lua = Get-Content $luaPath -Raw
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($lua))
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = 'script/campaign/mod/sects_m0.lua'; kind = @{ type = 'text'; format = 'Lua' } }
$null = Mcp-Tool 'write_file' @{ pack = $pack; path = 'script/campaign/mod/sects_m0.lua'; contents = @{ kind = 'raw'; base64 = $b64 } }
'added script/campaign/mod/sects_m0.lua'

# --- save + report -----------------------------------------------------------
$outDir = Join-Path $PSScriptRoot '..\build'
$null = New-Item -ItemType Directory -Force $outDir | Out-Null
$target = (Resolve-Path $outDir).Path + '\SectsCults_M0.pack'
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $target; disable_uuid_regeneration = $true }
"saved $target"

$diag = (Mcp-Tool 'run_diagnostics' @{}).structuredContent
"diagnostics: " + ($diag.result | ConvertTo-Json -Depth 6 -Compress)
if ($diag.result.by_level.error) {
    $rows = (Mcp-Tool 'list_diagnostics' @{ pack = $pack; levels = @('error'); limit = 20 }).structuredContent
    $rows.results | ForEach-Object { "[$($_.level)] $($_.path) - $($_.message)" }
}
