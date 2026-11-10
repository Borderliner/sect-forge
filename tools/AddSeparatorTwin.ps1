# Creates the <faction>_separatists twin that 3K expects for every real faction,
# links the pair through faction_civil_war_setups, and copies the donor's DB/startpos rows.
param(
    [string]$FactionKey = 'sects_white_lotus',
    [long]$FactionId = 1900000000,
    [string]$SepKey = 'sects_white_lotus_separatists',
    [long]$SepId = 1900000050,
    [string]$DonorPrimary = '3k_main_faction_zhai_rong',
    [string]$DonorSeparator = '3k_main_faction_zhai_rong_separatists',
    [long]$DonorSepId = 1570674463,
    [long]$SepIndex = 167374820
)

$ErrorActionPreference = 'Stop'
$db = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\raw_data\db'
$bak = 'C:\Users\Touraj\Projects\Sects&Cults\backup_ak_startpos'
$utf8 = New-Object System.Text.UTF8Encoding($false)

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

# 1. link rows for the separatist key (banners, name groups, stances, agents, groups...)
& (Join-Path $PSScriptRoot 'CloneFactionLinks.ps1') -DonorKey $DonorSeparator -FactionKey $SepKey | ForEach-Object { "  [sep] $_" }

# 2. DB faction row for the separatist (from the clean backup)
$path = Join-Path $db 'factions.xml'
$x = Read-XmlFile $path
$donorAll = [xml][IO.File]::ReadAllText((Join-Path $bak 'factions.xml'), [Text.Encoding]::UTF8)
$donor = $donorAll.dataroot.factions | Where-Object { $_.key -eq $DonorSeparator } | Select-Object -First 1
if (-not $donor) { throw "separatist donor $DonorSeparator not found" }
@($x.dataroot.factions | Where-Object { $_.key -eq $SepKey -or $_.record_key -eq $SepKey }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$rec = $x.ImportNode($donor.CloneNode($true), $true)
$rec.record_uuid = New-Guid
$rec.record_timestamp = Now-Ms
$rec.record_key = $SepKey
$rec.key = $SepKey
$rec.index = "$SepIndex"
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved separatist faction row $SepKey"

# 3. civil war setup linking the pair
$path = Join-Path $db 'faction_civil_war_setups.xml'
$x = Read-XmlFile $path
$donor = @($x.dataroot.faction_civil_war_setups | Where-Object { $_.primary_faction -eq $DonorPrimary }) | Select-Object -First 1
if (-not $donor) { throw "civil war donor for $DonorPrimary not found" }
@($x.dataroot.faction_civil_war_setups | Where-Object { $_.primary_faction -eq $FactionKey }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$rec = $donor.CloneNode($true)
$rec.record_uuid = New-Guid
$rec.record_timestamp = Now-Ms
$rec.record_key = $FactionKey
$rec.primary_faction = $FactionKey
$rec.secondary_faction = $SepKey
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved civil war setup $FactionKey -> $SepKey"

# 4. startpos faction row for the separatist (190 campaign)
$path = Join-Path $db 'start_pos_factions.xml'
$x = Read-XmlFile $path
@($x.dataroot.start_pos_factions | Where-Object { $_.ID -eq "$SepId" -or $_.faction -eq $SepKey }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$donorRow = $x.dataroot.start_pos_factions | Where-Object { $_.faction -eq $DonorSeparator -and $_.campaign -eq '3k_main_campaign_map' } | Select-Object -First 1
if (-not $donorRow) { throw "startpos separatist donor row not found" }
$rec = $x.ImportNode($donorRow.CloneNode($true), $true)
$rec.record_uuid = New-Guid
$rec.record_timestamp = Now-Ms
$rec.record_key = "$SepId"
$rec.ID = "$SepId"
$rec.faction = $SepKey
$rec.campaign = '3k_main_campaign_map'
$rec.playable = '0'
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved startpos faction row for $SepKey (id $SepId)"
