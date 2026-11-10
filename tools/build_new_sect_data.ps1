param(
    [string]$SectName = 'Way of the White Lotus',
    [string]$FactionKey = 'sects_white_lotus',
    [string]$PrimaryColour = '6A0DAD',
    [string]$SecondaryColour = 'E0C060',
    [string]$Forename = 'Wei',
    [string]$Surname = 'Yun',
    [bool]$IsMale = $true,
    [string]$ArtSet = '3k_main_art_set_generic_male_48_general_wood',
    [string]$Subtype = '3k_general_wood',
    [string]$Voiceover = 'vo_actor_group_generic_any_male_wood_general'
)

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\rpfm_mcp.ps1"

$slug = ($FactionKey -replace '^sects_', '')
$templateKey = "sects_${slug}_leader"
$forenameId = 1900000001
$surnameId = 1900000002

Start-Rpfm
Connect-Mcp
$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

$status = (Mcp-Tool 'session_status' @{}).structuredContent
$packName = "SectsCults_${slug}_Data.pack"
foreach ($p in $status.packs) {
    if ($p.name -eq $packName) { $null = Mcp-Tool 'close_pack' @{ pack = $p.key } }
}
$pack = (Mcp-Tool 'new_pack' @{ name = $packName }).structuredContent.key
"pack: $pack"

function New-DbTable([string]$path) {
    $tableName = ($path -split '/')[1]
    $null = Mcp-Tool 'create_file' @{ pack = $pack; path = $path; kind = @{ type = 'db'; table_name = $tableName } }
}
function Add-Rows([string]$path, $edits) {
    $r = Mcp-Tool 'edit_table' @{ pack = $pack; path = $path; edits = $edits }
    if ($r.isError) { throw ("Table write failed for $path : " + ($r.content.text -join ' ')) }
}

# --- new faction row, cloned from a small Han faction ------------------------
New-DbTable 'db/factions_tables/sects'
$donor = Mcp-Tool 'table_rows' @{
    file    = @{ source = 'game_files'; path = 'db/factions_tables/data__' }
    filters = @(@{ column = 'key'; op = 'equals'; value = '3k_main_faction_zhai_rong' })
}
if ($donor.structuredContent.total -lt 1) { throw 'Faction donor not found.' }
$donorIndex = $donor.structuredContent.rows[0].index

Add-Rows 'db/factions_tables/sects' @(
    @{ op = 'insert'; copy_from = @{ file = @{ source = 'game_files'; path = 'db/factions_tables/data__' }; index = $donorIndex }; values = @{
            key                       = $FactionKey
            screen_name               = "sects_screen_name_$slug"
            screen_adjective          = "sects_screen_adjective_$slug"
            category                  = 'playable'
            subculture                = '3k_main_chinese'
            feature_forest            = 'LOCKED_STANDARD'
            primary_colour_hex        = $PrimaryColour
            secondary_colour_hex      = $SecondaryColour
            uniform_colour_hex        = $PrimaryColour
            has_politics              = 0
        } }
)

# --- custom officer names ----------------------------------------------------
New-DbTable 'db/names_tables/sects'
Add-Rows 'db/names_tables/sects' @(
    @{ op = 'insert'; values = @{ names_group = 221; type = 1; gender = 2; frequency = 1; id = $forenameId } },
    @{ op = 'insert'; values = @{ names_group = 221; type = 1; gender = 2; frequency = 1; id = $surnameId } }
)

# --- custom officer template, cloned from a generic hero ---------------------
New-DbTable 'db/character_generation_templates_tables/sects'
$tplDonor = Mcp-Tool 'table_rows' @{
    file    = @{ source = 'game_files'; path = 'db/character_generation_templates_tables/data__' }
    filters = @(@{ column = 'key'; op = 'equals'; value = '3k_main_template_generated_ze_bu_hero_wood' })
}
if ($tplDonor.structuredContent.total -lt 1) { throw 'Template donor not found.' }
$tplIndex = $tplDonor.structuredContent.rows[0].index

Add-Rows 'db/character_generation_templates_tables/sects' @(
    @{ op = 'insert'; copy_from = @{ file = @{ source = 'game_files'; path = 'db/character_generation_templates_tables/data__' }; index = $tplIndex }; values = @{
            key          = $templateKey
            forename     = $forenameId
            family_name  = $surnameId
            clan_name    = 0
            other_name   = 0
            is_male      = $IsMale
            unique       = $true
            art_set_override = $ArtSet
            voiceover_actor  = $Voiceover
            subtype          = $Subtype
        } }
)

# --- sect belief -------------------------------------------------------------
New-DbTable 'db/effect_bundles_tables/sects'
New-DbTable 'db/effect_bundles_to_effects_junctions_tables/sects'
Add-Rows 'db/effect_bundles_tables/sects' @(
    @{ op = 'insert'; values = @{
            key                            = 'sects_white_lotus_belief'
            localised_description          = 'The White Lotus endures through discipline and quiet giving.'
            localised_title                = 'Belief: Asceticism'
            ui_icon                        = '.'
            bundle_target                  = 'faction'
            icon_supplement_colour_type    = 'positive'
            show_in_generic_bundle_display = $true
        } }
)
Add-Rows 'db/effect_bundles_to_effects_junctions_tables/sects' @(
    @{ op = 'insert'; values = @{
            effect_bundle_key = 'sects_white_lotus_belief'
            effect_key        = '3k_main_effect_economy_character_wealth_faction_background_income_mod'
            effect_scope      = 'faction_to_faction_own_unseen'
            value             = 5
            advancement_stage = 'start_turn_completed'
        } }
)

# --- localisation (all new keys) --------------------------------------------
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = 'text/db/sects_white_lotus__.loc'; kind = @{ type = 'loc' } }
Add-Rows 'text/db/sects_white_lotus__.loc' @(
    @{ op = 'insert'; values = @{ key = "sects_screen_name_$slug"; text = $SectName; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = "sects_screen_adjective_$slug"; text = $SectName; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = "names_name_$forenameId"; text = $Forename; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = "names_name_$surnameId"; text = $Surname; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = 'effect_bundles_localised_title_sects_white_lotus_belief'; text = 'Belief: Asceticism'; tooltip = $false } },
    @{ op = 'insert'; values = @{ key = 'effect_bundles_localised_description_sects_white_lotus_belief'; text = 'The White Lotus endures through discipline and quiet giving.'; tooltip = $false } }
)

$outDir = Join-Path $PSScriptRoot '..\build'
$null = New-Item -ItemType Directory -Force $outDir | Out-Null
$target = (Resolve-Path $outDir).Path + "\$packName"
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $target; disable_uuid_regeneration = $true }
"saved $target"

$diag = (Mcp-Tool 'run_diagnostics' @{}).structuredContent
"diagnostics: " + ($diag.result | ConvertTo-Json -Depth 6 -Compress)

[pscustomobject]@{
    Pack        = $target
    FactionKey  = $FactionKey
    TemplateKey = $templateKey
    ForenameId  = $forenameId
    SurnameId   = $surnameId
} | ConvertTo-Json -Depth 4
