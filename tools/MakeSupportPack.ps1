param(
    [string]$PackName = 'sects_white_lotus_support.pack',
    [string]$FactionKey = 'sects_white_lotus',
    [string]$SepKey = 'sects_white_lotus_separatists',
    [string]$SectName = 'Way of the White Lotus',
    [string]$Forename = 'Wei',
    [string]$Surname = 'Yun',
    [long]$ForenameId = 1900000001,
    [long]$SurnameId = 1900000002,
    [long]$FactionId = 1900000000,
    [long]$SepId = 1900000050,
    [switch]$InstallToGame,
    [string]$PackType = 'Mod'
)

# Bob's startpos build launches the game, and that game only sees the *vanilla*
# database unless our tables are loaded from data/. This pack carries the new
# faction's DB rows plus its localisation, saved as a Movie pack so it always
# loads - both for the build and (with the mod) for the player.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\rpfm_mcp.ps1"
Start-Rpfm
Connect-Mcp
$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

$tables = @(
    'factions',
    'names',
    'character_generation_templates',
    'character_generation_template_game_mode_details',
    'faction_banners',
    'unit_banners',
    'faction_uniform_colours',
    'faction_to_name_groups',
    'faction_agent_permitted_subtypes',
    'faction_civil_war_setups',
    'campaign_stances_factions_junctions',
    'campaign_map_attrition_faction_immunities',
    'campaign_group_member_criteria_factions',
    'building_chain_availabilities',
    'cai_personality_group_overrides',
    'effect_bundles',
    'effect_bundles_to_effects_junctions'
)

$status = (Mcp-Tool 'session_status' @{}).structuredContent
foreach ($p in $status.packs) { if ($p.name -eq $PackName) { $null = Mcp-Tool 'close_pack' @{ pack = $p.key } } }
$pack = (Mcp-Tool 'new_pack' @{ name = $PackName }).structuredContent.key
"pack: $pack"

foreach ($t in $tables) {
    $r = Mcp-Tool 'copy_files' @{ source = 'assembly_kit'; paths = @("db/${t}_tables/ak_data"); pack = $pack }
    if ($r.isError) {
        "  MISSING $t : " + (($r.structuredContent | ConvertTo-Json -Compress))
    } else {
        "  ok $t -> " + (($r.structuredContent.added) -join ',')
    }
}

# localisation
$locPath = 'text/db/sects_white_lotus__.loc'
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = $locPath; kind = @{ type = 'loc' } }
$rows = @(
    @{ key = "factions_screen_name_$FactionKey"; text = $SectName; tooltip = $false },
    @{ key = "factions_screen_adjective_$FactionKey"; text = $SectName; tooltip = $false },
    @{ key = "factions_screen_name_when_rebels_$FactionKey"; text = "$SectName Renegades"; tooltip = $false },
    @{ key = "factions_screen_name_$SepKey"; text = "$SectName Separatists"; tooltip = $false },
    @{ key = "factions_screen_adjective_$SepKey"; text = "$SectName Separatists"; tooltip = $false },
    @{ key = "factions_screen_name_when_rebels_$SepKey"; text = "$SectName Separatists"; tooltip = $false },
    @{ key = "names_name_$ForenameId"; text = $Forename; tooltip = $false },
    @{ key = "names_name_$SurnameId"; text = $Surname; tooltip = $false },
    @{ key = "start_pos_factions_description_$FactionId"; text = 'placeholder'; tooltip = $false },
    @{ key = "start_pos_factions_long_description_$FactionId"; text = 'placeholder'; tooltip = $false },
    @{ key = "start_pos_factions_description_$SepId"; text = 'placeholder'; tooltip = $false },
    @{ key = "start_pos_factions_long_description_$SepId"; text = 'placeholder'; tooltip = $false }
) | ForEach-Object { @{ op = 'insert'; values = $_ } }
$r = Mcp-Tool 'edit_table' @{ pack = $pack; path = $locPath; edits = $rows }
if ($r.isError) { throw ('loc write failed: ' + ($r.content.text -join ' ')) }
"  loc rows: $($rows.Count)"

$outDir = Join-Path $PSScriptRoot '..\build'
$null = New-Item -ItemType Directory -Force $outDir | Out-Null
$buildTarget = (Resolve-Path $outDir).Path + "\$PackName"
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $buildTarget; disable_uuid_regeneration = $true }
try { $null = Mcp-Tool 'update_pack' @{ pack = $pack; pack_type = $PackType } } catch { "update_pack failed: $($_.Exception.Message)" }
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $buildTarget; disable_uuid_regeneration = $true }
"saved $buildTarget"

if ($InstallToGame) {
    $gameTarget = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\data\' + $PackName
    Copy-Item $buildTarget $gameTarget -Force
    "installed $gameTarget"
}
