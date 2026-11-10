param(
    [string]$FactionKey = 'sects_white_lotus',
    [string]$SepKey = 'sects_white_lotus_separatists',
    [string]$SectName = 'Way of the White Lotus',
    [string]$Forename = 'Wei',
    [string]$Surname = 'Yun',
    [long]$ForenameId = 1900000001,
    [long]$SurnameId = 1900000002,
    [long]$FactionId = 1900000000,
    [long]$SepId = 1900000050,
    [switch]$Restore
)

# Installs the sect straight into the game's own packs (no mod manager involved):
#   data/database.pack  <- faction, officer template, links, frontend entries
#   data/local_en.pack  <- localisation
#   data/campaigns/...  <- built startpos (already written by the build)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\rpfm_mcp.ps1"
$game = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS'
$dbPack = Join-Path $game 'data\database.pack'
$locPack = Join-Path $game 'data\local_en.pack'
$akdb = Join-Path $game 'assembly_kit\raw_data\db'

Start-Rpfm
Connect-Mcp
$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

if ($Restore) {
    Get-Process Three_Kingdoms -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    foreach ($f in @($dbPack, $locPack)) {
        if (Test-Path "$f.bak_sects") { Copy-Item "$f.bak_sects" $f -Force; "restored $(Split-Path $f -Leaf)" }
    }
    Get-ChildItem (Join-Path $game 'data') -File -Filter 'SectsCults_white_lotus.pack' -ErrorAction SilentlyContinue | Remove-Item -Force
    return
}

foreach ($f in @($dbPack, $locPack)) { if (-not (Test-Path "$f.bak_sects")) { Copy-Item $f "$f.bak_sects" -Force; "backed up $(Split-Path $f -Leaf)" } }
Get-Process Three_Kingdoms -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

$tables = @(
    'factions','names','character_generation_templates','character_generation_template_game_mode_details',
    'faction_banners','unit_banners','faction_uniform_colours','faction_to_name_groups',
    'faction_agent_permitted_subtypes','faction_civil_war_setups','campaign_stances_factions_junctions',
    'campaign_map_attrition_faction_immunities','campaign_group_member_criteria_factions',
    'building_chain_availabilities','cai_personality_group_overrides',
    'frontend_characters','frontend_factions','frontend_faction_leaders','frontend_faction_to_frontend_faction_leaders'
)
$matchColumns = @{
    'factions' = 'key'; 'names' = 'id'; 'character_generation_templates' = 'key'
    'character_generation_template_game_mode_details' = 'character_generation_template'
    'faction_banners' = 'key'; 'unit_banners' = 'key'; 'faction_uniform_colours' = 'faction_name'
    'faction_to_name_groups' = 'faction'; 'faction_agent_permitted_subtypes' = 'faction'
    'faction_civil_war_setups' = 'primary_faction'; 'campaign_stances_factions_junctions' = 'faction'
    'campaign_map_attrition_faction_immunities' = 'faction'; 'campaign_group_member_criteria_factions' = 'faction'
    'building_chain_availabilities' = 'faction'; 'cai_personality_group_overrides' = 'faction_key'
    'frontend_characters' = 'key'; 'frontend_factions' = 'faction'; 'frontend_faction_leaders' = 'key'
    'frontend_faction_to_frontend_faction_leaders' = 'frontend_faction'
}
$needles = @($FactionKey, $SepKey, "${FactionKey}_ruler", "${FactionKey}_leader")

$identityOverrides = @{
    'factions' = @{
        'screen_name' = 'sects_screen_name_white_lotus'; 'screen_adjective' = 'sects_screen_adjective_white_lotus'
        'screen_name_when_rebels' = 'sects_screen_name_when_rebels_white_lotus'
        'attack_desc' = 'sects_faction_attack_desc'; 'defend_desc' = 'sects_faction_defend_desc'
    }
    'frontend_faction_leaders' = @{
        'localised_subtitle' = 'sects_fe_subtitle'; 'localised_description' = 'sects_fe_description'
        'localised_difficulty' = 'sects_fe_difficulty'; 'localised_action_points' = 'sects_fe_action_points'
        'loading_screen_text' = 'sects_fe_loading_text'
    }
}
$sepOverrides = @{
    'factions' = @{
        'screen_name' = 'sects_screen_name_white_lotus_sep'; 'screen_adjective' = 'sects_screen_adjective_white_lotus_sep'
        'screen_name_when_rebels' = 'sects_screen_name_when_rebels_white_lotus_sep'
        'attack_desc' = 'sects_faction_attack_desc'; 'defend_desc' = 'sects_faction_defend_desc'
    }
}

$status = (Mcp-Tool 'session_status' @{}).structuredContent
foreach ($o in $status.packs) {
    if ($o.path -and (($o.path -replace '\\', '/') -match 'database\.pack$' -or ($o.path -replace '\\', '/') -match 'local_en\.pack$')) { $null = Mcp-Tool 'close_pack' @{ pack = $o.key } }
}
$p = (Mcp-Tool 'open_pack' @{ paths = @($dbPack) }).structuredContent
$key = $p.key
"database.pack open: $($p.file_count) files"

$total = 0
foreach ($table in $tables) {
    if ($table -eq 'names') {
        $edits = @(
            @{ op = 'insert'; values = @{ names_group = '221'; type = '0'; gender = '0'; frequency = '1'; id = "$ForenameId" } },
            @{ op = 'insert'; values = @{ names_group = '221'; type = '1'; gender = '2'; frequency = '1'; id = "$SurnameId" } }
        )
        $r = Mcp-Tool 'edit_table' @{ pack = $key; path = 'db/names_tables/data__'; edits = $edits }
        if (-not $r.isError) { $total += 2 }
        continue
    }
    $xmlPath = Join-Path $akdb "$table.xml"
    if (-not (Test-Path $xmlPath)) { continue }
    $ti = Mcp-Tool 'table_info' @{ file = @{ source = @{ pack = $key }; path = "db/${table}_tables/data__" } }
    $cols = @($ti.structuredContent.columns); $colType = @{}
    foreach ($c in $cols) { $colType[$c.name] = $c.field_type }
    $doc = New-Object System.Xml.XmlDocument
    $doc.LoadXml([IO.File]::ReadAllText($xmlPath, [Text.Encoding]::UTF8))
    $records = @($doc.dataroot.ChildNodes | Where-Object { $_.NodeType -eq 'Element' -and $_.Name -eq $table })
    $edits = @()
    foreach ($rec in $records) {
        $akVals = @{}
        foreach ($child in $rec.ChildNodes) { if ($child.NodeType -eq 'Element') { $akVals[$child.Name] = $child.InnerText } }
        $hit = $false
        foreach ($n in $needles) { foreach ($v in $akVals.Values) { if ("$v" -eq $n) { $hit = $true; break } }; if ($hit) { break } }
        if (-not $hit) { continue }
        $vals = @{}
        foreach ($cname in $colType.Keys) {
            if (-not $akVals.ContainsKey($cname)) { continue }
            $v = "$($akVals[$cname])"; $t = $colType[$cname]
            if ($v -eq '' -and $t -notmatch 'String') { $v = if ($t -match 'Bool') { 'false' } else { '0' } }
            $vals[$cname] = $v
        }
        $isSep = ("$($vals['faction'])" -like '*_separatists') -or ("$($vals['key'])" -like '*_separatists') -or ("$($vals['faction_name'])" -like '*_separatists')
        $ov = if ($isSep -and $sepOverrides.ContainsKey($table)) { $sepOverrides[$table] } elseif ($identityOverrides.ContainsKey($table)) { $identityOverrides[$table] } else { @{} }
        foreach ($k in $ov.Keys) { if ($colType.ContainsKey($k)) { $vals[$k] = $ov[$k] } }
        $edits += @{ op = 'insert'; values = $vals }
    }
    if ($edits.Count -eq 0) { continue }
    $r = Mcp-Tool 'edit_table' @{ pack = $key; path = "db/${table}_tables/data__"; edits = $edits }
    if ($r.isError) { "  $table FAILED " + (($r.content | ForEach-Object { $_.text }) -join ' ') } else { "  $table +$($edits.Count)"; $total += $edits.Count }
}
$r = Mcp-Tool 'save_pack' @{ pack = $key; path = $dbPack; allow_editing_ca_packs = $true; disable_uuid_regeneration = $true }
if ($r.isError) { throw ('database.pack save failed: ' + (($r.content | ForEach-Object { $_.text }) -join ' ')) }
"database.pack: $total rows"

# localisation
$locPath = 'text/db/sects_white_lotus__.loc'
$lp = (Mcp-Tool 'open_pack' @{ paths = @($locPack) }).structuredContent
$lkey = $lp.key
$null = Mcp-Tool 'create_file' @{ pack = $lkey; path = $locPath; kind = @{ type = 'loc' } }
$locRows = @(
    @{ key = 'sects_screen_name_white_lotus'; text = $SectName },
    @{ key = 'sects_screen_adjective_white_lotus'; text = $SectName },
    @{ key = 'sects_screen_name_when_rebels_white_lotus'; text = "$SectName Renegades" },
    @{ key = 'sects_screen_name_white_lotus_sep'; text = "$SectName Separatists" },
    @{ key = 'sects_screen_adjective_white_lotus_sep'; text = "$SectName Separatists" },
    @{ key = 'sects_screen_name_when_rebels_white_lotus_sep'; text = "$SectName Separatists" },
    @{ key = 'sects_faction_attack_desc'; text = "You are attacking a $SectName force!" },
    @{ key = 'sects_faction_defend_desc'; text = "You are defending against a $SectName force!" },
    @{ key = 'sects_fe_subtitle'; text = $SectName },
    @{ key = 'sects_fe_description'; text = "$SectName begins with a single city and everything to prove." },
    @{ key = 'sects_fe_difficulty'; text = 'Starting situation: Easy' },
    @{ key = 'sects_fe_action_points'; text = 'placeholder' },
    @{ key = 'sects_fe_loading_text'; text = $SectName },
    @{ key = "names_name_$ForenameId"; text = $Forename },
    @{ key = "names_name_$SurnameId"; text = $Surname },
    @{ key = "start_pos_factions_description_$FactionId"; text = $SectName },
    @{ key = "start_pos_factions_long_description_$FactionId"; text = $SectName },
    @{ key = "start_pos_factions_description_$SepId"; text = "$SectName Separatists" },
    @{ key = "start_pos_factions_long_description_$SepId"; text = "$SectName Separatists" }
) | ForEach-Object { @{ op = 'insert'; values = @{ key = $_.key; text = $_.text; tooltip = $false } } }
$r = Mcp-Tool 'edit_table' @{ pack = $lkey; path = $locPath; edits = $locRows }
if ($r.isError) { "loc FAILED " + (($r.content | ForEach-Object { $_.text }) -join ' ') } else { "loc +$($locRows.Count)" }
$r = Mcp-Tool 'save_pack' @{ pack = $lkey; path = $locPack; allow_editing_ca_packs = $true; disable_uuid_regeneration = $true }
if ($r.isError) { throw ('local_en.pack save failed: ' + (($r.content | ForEach-Object { $_.text }) -join ' ')) }
"local_en.pack patched"
