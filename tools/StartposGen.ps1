param(
    [string]$SectName = 'Way of the White Lotus',
    [string]$FactionKey = 'sects_white_lotus',
    [long]$FactionId = 1900000000,
    [long]$LeaderId = 1900000100,
    [long]$SettlementRowId = 1900000200,
    [string]$TemplateKey = 'sects_white_lotus_leader',
    [string]$LeaderTemplate = '',
    [string]$CityRegion = '3k_main_jianan_capital',
    [long]$CityRegionId = 2130431103,
    [double]$StartX = 620,
    [double]$StartY = 390,
    [switch]$SkipLeader,
    [switch]$SkipRetinue,
    [switch]$SkipFactionRow,
    [switch]$SkipRegion,
    [switch]$SkipSettlement,
    [string]$Playable = '1',
    [string]$CeoInitialData = '3k_main_ceo_initial_data_faction_generic'
)

$ErrorActionPreference = 'Stop'
$db = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\raw_data\db'

function New-RecordGuid { return '{' + [guid]::NewGuid().ToString() + '}' }
function Now-Ms { return [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() }
function Read-XmlFile([string]$p) {
    # The AK startpos XMLs are UTF-8 without BOM: read them as UTF-8, never as ANSI,
    # otherwise every Chinese name in the table gets mangled on the round trip.
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

# --- faction ----------------------------------------------------------------
$path = Join-Path $db 'start_pos_factions.xml'
$x = (Read-XmlFile $path)
if ($SkipFactionRow) {
    # reuse a faction row that already exists in this campaign (e.g. a dormant vanilla slot)
    $existing = $x.dataroot.start_pos_factions | Where-Object { $_.faction -eq $FactionKey -and $_.campaign -eq '3k_main_campaign_map' } | Select-Object -First 1
    if (-not $existing) { throw "no existing startpos faction row for $FactionKey" }
    $existing.playable = $Playable
    $FactionId = [long]$existing.ID
    Save-XmlFile $x $path
    "reused faction row $FactionId ($FactionKey), playable=$Playable"
} else {
$donor = $x.dataroot.start_pos_factions | Where-Object { $_.ID -eq '1570674462' } | Select-Object -First 1
if (-not $donor) { throw 'Faction donor (Zhai Rong) not found.' }
# idempotent: drop any previous record for this faction id/key
$x.dataroot.start_pos_factions | Where-Object { $_.ID -eq "$FactionId" -or $_.faction -eq $FactionKey } | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$rec = $donor.CloneNode($true)
$rec.record_uuid = New-RecordGuid
$rec.record_timestamp = "$(Now-Ms)"
$rec.record_key = "$FactionId"
$rec.ID = "$FactionId"
$rec.faction = $FactionKey
$rec.campaign = '3k_main_campaign_map'
$rec.playable = $Playable
$rec.treasury = '2000'
$rec.ceo_initial_data = $CeoInitialData
$rec.starting_order = '200'
$rec.is_major = '0'
$rec.unique = '0'
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved faction $FactionId"
}

# --- leader -----------------------------------------------------------------
if ($SkipLeader) {
    "skipped leader"
} else {
    $path = Join-Path $db 'start_pos_characters.xml'
    $x = (Read-XmlFile $path)
    $donor = $x.dataroot.start_pos_characters | Where-Object { $_.ID -eq '1577879871' } | Select-Object -First 1
    if (-not $donor) { throw 'Character donor (Zhai Rong leader) not found.' }
    $x.dataroot.start_pos_characters | Where-Object { $_.ID -eq "$LeaderId" } | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
    $rec = $donor.CloneNode($true)
    $rec.record_uuid = New-RecordGuid
    $rec.record_timestamp = "$(Now-Ms)"
    $rec.record_key = "$LeaderId"
    $rec.ID = "$LeaderId"
    $rec.faction = "$FactionId"
    if ($LeaderTemplate) { $rec.template = $LeaderTemplate } else { $rec.template = $TemplateKey }
    $rec.ministerial_position = 'faction_leader'
    $rec.start_on_map = '1'
    $rec.startx = "$StartX"
    $rec.starty = "$StartY"
    $rec.progenitor = '1'
    $rec.rank_override = '-1'
    [void]$x.dataroot.AppendChild($rec)
    Save-XmlFile $x $path
    "saved leader $LeaderId"

    # --- retinue slot modifiers for the leader ------------------------------
    if ($SkipRetinue) {
        "skipped retinue modifiers"
    } else {
    $path = Join-Path $db 'start_pos_character_retinue_unit_modifiers.xml'
    $x = (Read-XmlFile $path)
    $donorRows = $x.dataroot.start_pos_character_retinue_unit_modifiers | Where-Object { $_.character -eq '1577879871' }
    $x.dataroot.start_pos_character_retinue_unit_modifiers | Where-Object { $_.character -eq "$LeaderId" } | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
    foreach ($d in $donorRows) {
        $rec = $d.CloneNode($true)
        $rec.record_uuid = New-RecordGuid
        $rec.record_timestamp = "$(Now-Ms)"
        $rec.record_key = "$LeaderId" + $rec.retinue_slot_index
        $rec.character = "$LeaderId"
        [void]$x.dataroot.AppendChild($rec)
    }
    Save-XmlFile $x $path
    "added retinue modifiers"
    }
}

# --- city ownership ---------------------------------------------------------
if ($SkipRegion) {
    "skipped region ownership"
} else {
    $path = Join-Path $db 'start_pos_regions.xml'
    $x = (Read-XmlFile $path)
    $reg = $x.dataroot.start_pos_regions | Where-Object {
        $_.region -eq $CityRegion -and $_.campaign -eq '3k_main_campaign_map'
    } | Select-Object -First 1
    if (-not $reg) { throw "City region $CityRegion not found in 190." }
    $reg.owning_faction = "$FactionId"
    $reg.faction_capital = '1'
    Save-XmlFile $x $path
    "saved region ownership $CityRegion -> $FactionId"

    if ($SkipSettlement) {
        "skipped settlement"
    } else {
        $path = Join-Path $db 'start_pos_settlements.xml'
        $x = (Read-XmlFile $path)
        $existing = $x.dataroot.start_pos_settlements | Where-Object { $_.region -eq "$CityRegionId" } | Select-Object -First 1
        if ($existing) {
            $existing.primary_building = '3k_city_1'
            "updated existing settlement row for $CityRegionId"
        } else {
            $donor = $x.dataroot.start_pos_settlements | Where-Object { $_.settlement_id -eq "settlement:$CityRegion" -and $_.region -eq '597803789' } | Select-Object -First 1
            if (-not $donor) { throw 'Settlement donor row not found.' }
            $rec = $donor.CloneNode($true)
            $rec.record_uuid = New-RecordGuid
            $rec.record_timestamp = "$(Now-Ms)"
            $rec.record_key = "settlement:${CityRegion}${CityRegionId}"
            $rec.region = "$CityRegionId"
            $rec.id = "$SettlementRowId"
            $rec.primary_building = '3k_city_1'
            [void]$x.dataroot.AppendChild($rec)
            "added settlement row for $CityRegionId"
        }
        Save-XmlFile $x $path
    }
}

[pscustomobject]@{
    FactionId = $FactionId
    LeaderId = $LeaderId
    City = $CityRegion
    Coords = "$StartX,$StartY"
} | ConvertTo-Json -Compress


