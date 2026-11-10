param(
    [string]$TargetFactionId = '101052446',
    [long]$NewCharId = 1900000100,
    [string]$Template = '3k_dlc07_template_generated_chen_bai_hero_earth',
    [string]$CityRegion = '3k_main_hepu_capital',
    [long]$CityRegionId = 2130431098,
    [double]$StartX = 316,
    [double]$StartY = 157
)

$ErrorActionPreference = 'Stop'
$db = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\raw_data\db'

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

# --- region ownership -------------------------------------------------------
$path = Join-Path $db 'start_pos_regions.xml'
$x = Read-XmlFile $path
$reg = $x.dataroot.start_pos_regions | Where-Object { $_.region -eq $CityRegion -and $_.campaign -eq '3k_main_campaign_map' } | Select-Object -First 1
if (-not $reg) { throw "region $CityRegion not found" }
$reg.owning_faction = $TargetFactionId
$reg.faction_capital = '1'
Save-XmlFile $x $path
"region $CityRegion -> faction $TargetFactionId"

# --- leader character -------------------------------------------------------
$path = Join-Path $db 'start_pos_characters.xml'
$x = Read-XmlFile $path
$donor = $x.dataroot.start_pos_characters | Where-Object { $_.ID -eq '1577879871' } | Select-Object -First 1
if (-not $donor) { throw 'donor character not found' }
@($x.dataroot.start_pos_characters | Where-Object { $_.ID -eq "$NewCharId" }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$rec = $donor.CloneNode($true)
$rec.record_uuid = '{' + [guid]::NewGuid().ToString() + '}'
$rec.record_timestamp = "$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())"
$rec.record_key = "$NewCharId"
$rec.ID = "$NewCharId"
$rec.faction = $TargetFactionId
$rec.template = $Template
$rec.ministerial_position = 'faction_leader'
$rec.start_on_map = '1'
$rec.startx = "$StartX"
$rec.starty = "$StartY"
$rec.progenitor = '1'
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"added leader $NewCharId (template $Template) to faction $TargetFactionId"

# --- retinue slot modifiers -------------------------------------------------
$path = Join-Path $db 'start_pos_character_retinue_unit_modifiers.xml'
$x = Read-XmlFile $path
$donorRows = @($x.dataroot.start_pos_character_retinue_unit_modifiers | Where-Object { $_.character -eq '1577879871' })
@($x.dataroot.start_pos_character_retinue_unit_modifiers | Where-Object { $_.character -eq "$NewCharId" }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
foreach ($d in $donorRows) {
    $c = $d.CloneNode($true)
    $c.record_uuid = '{' + [guid]::NewGuid().ToString() + '}'
    $c.record_timestamp = "$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())"
    $c.record_key = "$NewCharId" + $c.retinue_slot_index
    $c.character = "$NewCharId"
    [void]$x.dataroot.AppendChild($c)
}
Save-XmlFile $x $path
"added $($donorRows.Count) retinue modifiers"
