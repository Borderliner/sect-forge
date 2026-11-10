param(
    [switch]$Restore
)

# Injects the new faction's rows into data/database.pack (the retail DB the
# Assembly Kit's build-time game reads), mapping only columns that exist in the
# retail table version. Use -Restore to put the original pack back.
$ErrorActionPreference = 'Stop'
$game = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS'
$packPath = Join-Path $game 'data\database.pack'
$backup = "$packPath.bak_sects"
$akdb = Join-Path $game 'assembly_kit\raw_data\db'

if ($Restore) {
    Get-Process Three_Kingdoms, bob.retail.x64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    if (-not (Test-Path $backup)) { throw 'no backup' }
    Copy-Item $backup $packPath -Force
    "restored database.pack"
    return
}

. "$PSScriptRoot\rpfm_mcp.ps1"
Start-Rpfm
Connect-Mcp
$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

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
$needles = @('sects_white_lotus', 'sects_white_lotus_separatists', 'sects_white_lotus_ruler', 'sects_white_lotus_leader', '1900000001', '1900000002')

$status = (Mcp-Tool 'session_status' @{}).structuredContent
foreach ($open in $status.packs) {
    if ($open.path -and ($open.path -replace '\\', '/') -match 'database\.pack$') { $null = Mcp-Tool 'close_pack' @{ pack = $open.key } }
}
$p = (Mcp-Tool 'open_pack' @{ paths = @($packPath) }).structuredContent
$key = $p.key
"open: $($p.file_count) files"

$total = 0
foreach ($table in $tables) {
    $xmlPath = Join-Path $akdb "$table.xml"
    if (-not (Test-Path $xmlPath)) { continue }

    # columns the retail table expects
    $ti = Mcp-Tool 'table_info' @{ file = @{ source = @{ pack = $key }; path = "db/${table}_tables/data__" } }
    $cols = @($ti.structuredContent.columns)
    $colType = @{}
    foreach ($c in $cols) { $colType[$c.name] = $c.field_type }

    $doc = New-Object System.Xml.XmlDocument
    $doc.LoadXml([IO.File]::ReadAllText($xmlPath, [Text.Encoding]::UTF8))
    $records = @($doc.dataroot.ChildNodes | Where-Object { $_.NodeType -eq 'Element' -and $_.Name -eq $table })
    $edits = @()
    foreach ($rec in $records) {
        $akVals = @{}
        foreach ($child in $rec.ChildNodes) { if ($child.NodeType -eq 'Element') { $akVals[$child.Name] = $child.InnerText } }
        $hit = $false
        foreach ($n in $needles) {
            foreach ($v in $akVals.Values) { if ("$v" -eq $n) { $hit = $true; break } }
            if ($hit) { break }
        }
        if (-not $hit) { continue }

        $vals = @{}
        foreach ($cname in $colType.Keys) {
            if (-not $akVals.ContainsKey($cname)) { continue }
            $v = "$($akVals[$cname])"
            $t = $colType[$cname]
            if ($v -eq '' -and $t -notmatch 'String') {
                if ($t -match 'Bool') { $v = 'false' } else { $v = '0' }
            }
            $vals[$cname] = $v
        }
        $edits += @{ op = 'insert'; values = $vals }
    }
    if ($edits.Count -eq 0) { "  $table : nothing"; continue }
    $r = Mcp-Tool 'edit_table' @{ pack = $key; path = "db/${table}_tables/data__"; edits = $edits }
    if ($r.isError) { "  $table : FAILED " + (($r.content | ForEach-Object { $_.text }) -join ' ') }
    else { "  $table : +$($edits.Count)"; $total += $edits.Count }
}

$r = Mcp-Tool 'save_pack' @{ pack = $key; path = $packPath; allow_editing_ca_packs = $true; disable_uuid_regeneration = $true }
if ($r.isError) { "SAVE ERROR: " + (($r.content | ForEach-Object { $_.text }) -join ' ') } else { "saved database.pack ($total rows)" }
