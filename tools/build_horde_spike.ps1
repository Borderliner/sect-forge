# Builds the wander/regionless spike pack:
#  - grants regionless recruiting to every faction in the 190 campaign
#  - adds a replenishment bundle for regionless forces
#  - strips the player's regions on first tick and keeps the faction alive
. "$PSScriptRoot\rpfm_mcp.ps1"

$ErrorActionPreference = 'Stop'
Start-Rpfm
Connect-Mcp

$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

$status = (Mcp-Tool 'session_status' @{}).structuredContent
foreach ($p in $status.packs) {
    if ($p.name -eq 'SectsCults_HordeSpike.pack') { $null = Mcp-Tool 'close_pack' @{ pack = $p.key } }
}

$pack = (Mcp-Tool 'new_pack' @{ name = 'SectsCults_HordeSpike.pack' }).structuredContent.key
"pack: $pack"

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

# --- regionless recruiting feature for the 190 campaign -----------------------
New-DbTable 'db/campaign_features_tables/sects_cults'
New-DbTable 'db/campaign_group_member_criteria_campaigns_tables/sects_cults'
New-DbTable 'db/effect_bundles_tables/sects_cults'
New-DbTable 'db/effect_bundles_to_effects_junctions_tables/sects_cults'

Add-Rows 'db/campaign_features_tables/sects_cults' @(
    @{ op = 'insert'; values = @{
            available = $true
            feature   = 'can_recruit_in_non_hostile_territory_when_regionless'
            group     = 'sects_regionless'
        } }
)
Add-Rows 'db/campaign_group_member_criteria_campaigns_tables/sects_cults' @(
    @{ op = 'insert'; values = @{ campaign = '3k_main_campaign_map'; member = 'sects_regionless' } },
    # Reuse the vanilla DLC04/DLC05 groups that already carry the regionless-recruit feature.
    @{ op = 'insert'; values = @{ campaign = '3k_main_campaign_map'; member = '3k_dlc04_campaign_group_can_recruit_while_regionless' } },
    @{ op = 'insert'; values = @{ campaign = '3k_main_campaign_map'; member = '3k_dlc05_campaign_group_member_can_recruit_while_regionless_sun_ce' } }
)

# --- replenishment bundle ----------------------------------------------------
Add-Rows 'db/effect_bundles_tables/sects_cults' @(
    @{ op = 'insert'; values = @{
            key                            = 'sects_regionless_replenish'
            localised_description          = 'Sect warbands recover away from their own lands.'
            localised_title                = 'Wandering Sect'
            ui_icon                        = 'replenishment.png'
            bundle_target                  = 'faction'
            icon_supplement_colour_type    = 'positive'
            show_in_generic_bundle_display = $true
        } }
)
Add-Rows 'db/effect_bundles_to_effects_junctions_tables/sects_cults' @(
    @{ op = 'insert'; values = @{
            effect_bundle_key = 'sects_regionless_replenish'
            effect_key        = '3k_main_effect_forces_replenishment_foreign_land'
            effect_scope      = 'faction_to_character_own_unseen'
            value             = 10
            advancement_stage = 'start_turn_completed'
        } },
    @{ op = 'insert'; values = @{
            effect_bundle_key = 'sects_regionless_replenish'
            effect_key        = '3k_main_effect_replenishment_faction'
            effect_scope      = 'faction_to_force_own_unseen'
            value             = 5
            advancement_stage = 'start_turn_completed'
        } }
)

# --- localisation ------------------------------------------------------------
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = 'text/db/sects_wander.loc'; kind = @{ type = 'loc' } }
Add-Rows 'text/db/sects_wander.loc' @(
    @{ op = 'insert'; values = @{ key = 'effect_bundles_localised_title_sects_regionless_replenish'; text = 'Wandering Sect'; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = 'effect_bundles_localised_description_sects_regionless_replenish'; text = 'Sect warbands recover away from their own lands.'; tooltip = $false } }
)

# --- script ------------------------------------------------------------------
$lua = Get-Content (Join-Path $PSScriptRoot '..\src\script\sects_horde_spike.lua') -Raw
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($lua))
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = 'script/campaign/mod/sects_horde_spike.lua'; kind = @{ type = 'text'; format = 'Lua' } }
$null = Mcp-Tool 'write_file' @{ pack = $pack; path = 'script/campaign/mod/sects_horde_spike.lua'; contents = @{ kind = 'raw'; base64 = $b64 } }

$outDir = Join-Path $PSScriptRoot '..\build'
$null = New-Item -ItemType Directory -Force $outDir | Out-Null
$target = (Resolve-Path $outDir).Path + '\SectsCults_HordeSpike.pack'
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $target; disable_uuid_regeneration = $true }
"saved $target"

$diag = (Mcp-Tool 'run_diagnostics' @{}).structuredContent
"diagnostics: " + ($diag.result | ConvertTo-Json -Depth 6 -Compress)
