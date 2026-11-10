# A faction can only be playable if the campaign-selection chain exists:
#   frontend_factions -> frontend_faction_to_frontend_faction_leaders
#                     -> frontend_faction_leaders -> frontend_characters
# Without it, start_pos_factions.playable = 1 makes the startpos build crash.
param(
    [string]$FactionKey = '3k_main_faction_wuwei',
    [string]$TemplateKey = 'sects_white_lotus_leader',
    [string]$SectName = 'Way of the White Lotus',
    [string]$LeaderKey = 'sects_white_lotus_ruler',
    [string]$DonorFaction = '3k_main_faction_tao_qian',
    [int]$SortOrder = 60,
    [int]$ActiveStart = 182,
    [int]$ActiveEnd = 194,
    [string]$GeneralType = '3k_main_hero_wood',
    [double]$StartX = 316,
    [double]$StartY = 157
)

$ErrorActionPreference = 'Stop'
$db = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\raw_data\db'

function New-Guid { '{' + [guid]::NewGuid().ToString() + '}' }
function Now-Ms { "$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())" }
function Read-XmlFile([string]$p) {
    $doc = New-Object System.Xml.XmlDocument
    $doc.PreserveWhitespace = $true
    $doc.LoadXml([IO.File]::ReadAllText($p, [Text.Encoding]::UTF8))
    return $doc
}
function Save-XmlFile($doc, [string]$p) {
    $settings = New-Object System.Xml.XmlWriterSettings
    $settings.Encoding = New-Object System.Text.UTF8Encoding($false)
    $settings.Indent = $false
    $writer = [System.Xml.XmlWriter]::Create($p, $settings)
    try { $doc.Save($writer) } finally { $writer.Close() }
}
function Set-Text($el, [string]$text) {
    if ($el) { $el.InnerText = $text; try { $el.SetAttribute('last_approved_text', $text) } catch { } }
}

# --- frontend_characters ----------------------------------------------------
$path = Join-Path $db 'frontend_characters.xml'
$x = Read-XmlFile $path
$donor = $x.dataroot.frontend_characters | Where-Object { $_.character_generation_template -eq '3k_main_template_historical_tao_qian_hero_water' } | Select-Object -First 1
if (-not $donor) { $donor = $x.dataroot.frontend_characters | Select-Object -First 1 }
$feCharKey = "${TemplateKey}_faction_leader"
@($x.dataroot.frontend_characters | Where-Object { $_.key -eq $feCharKey }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$rec = $donor.CloneNode($true)
$rec.SetAttribute('record_uuid', [string](New-Guid)); $rec.SetAttribute('record_timestamp', [string](Now-Ms))
$rec.record_key = $feCharKey; $rec.key = $feCharKey
$rec.character_generation_template = $TemplateKey
$rec.ministerial_position = 'faction_leader'
$rec.optional_general_type = $GeneralType
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved frontend character $feCharKey"

# --- frontend_faction_leaders ----------------------------------------------
$path = Join-Path $db 'frontend_faction_leaders.xml'
$x = Read-XmlFile $path
$donor = $x.dataroot.frontend_faction_leaders | Where-Object { $_.key -eq '3k_main_political_party_tao_qian_ruler' } | Select-Object -First 1
if (-not $donor) { throw 'frontend leader donor not found' }
@($x.dataroot.frontend_faction_leaders | Where-Object { $_.key -eq $LeaderKey }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$rec = $donor.CloneNode($true)
$rec.SetAttribute('record_uuid', [string](New-Guid)); $rec.SetAttribute('record_timestamp', [string](Now-Ms))
$rec.record_key = $LeaderKey; $rec.key = $LeaderKey
$rec.frontend_character = $feCharKey
$rec.sort_order = "$SortOrder"
$rec.override_force_location_x = "$StartX"
$rec.override_force_location_y = "$StartY"
Set-Text $rec.localised_subtitle $SectName
Set-Text $rec.localised_description "$SectName begins as a small fellowship with one city and everything to prove."
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved frontend leader $LeaderKey"

# --- frontend_factions ------------------------------------------------------
$path = Join-Path $db 'frontend_factions.xml'
$x = Read-XmlFile $path
$donor = $x.dataroot.frontend_factions | Where-Object { $_.faction -eq $DonorFaction } | Select-Object -First 1
if (-not $donor) { throw "frontend faction donor $DonorFaction not found" }
@($x.dataroot.frontend_factions | Where-Object { $_.faction -eq $FactionKey }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$rec = $donor.CloneNode($true)
$rec.SetAttribute('record_uuid', [string](New-Guid)); $rec.SetAttribute('record_timestamp', [string](Now-Ms))
$rec.record_key = $FactionKey
$rec.faction = $FactionKey
$rec.sort_order = "$SortOrder"
$rec.active_years_start = "$ActiveStart"
$rec.active_years_end = "$ActiveEnd"
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved frontend faction $FactionKey (playstyle $($rec.playstyle))"

# --- link faction -> leader for the 190 campaign ----------------------------
$path = Join-Path $db 'frontend_faction_to_frontend_faction_leaders.xml'
$x = Read-XmlFile $path
$donor = $x.dataroot.frontend_faction_to_frontend_faction_leaders | Where-Object { $_.frontend_faction -eq $DonorFaction -and $_.campaign_key -eq '3k_main_campaign_map' } | Select-Object -First 1
if (-not $donor) { throw 'frontend link donor not found' }
@($x.dataroot.frontend_faction_to_frontend_faction_leaders | Where-Object { $_.frontend_faction -eq $FactionKey -and $_.campaign_key -eq '3k_main_campaign_map' }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$rec = $donor.CloneNode($true)
$rec.SetAttribute('record_uuid', [string](New-Guid)); $rec.SetAttribute('record_timestamp', [string](Now-Ms))
$rec.record_key = $FactionKey + $LeaderKey + '3k_main_campaign_map'
$rec.campaign_key = '3k_main_campaign_map'
$rec.frontend_faction = $FactionKey
$rec.frontend_faction_leader = $LeaderKey
$rec.is_default = '1'
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved frontend link for 190"
