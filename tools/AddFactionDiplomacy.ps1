# Gives the new faction the same starting diplomatic relation that every landed
# vanilla faction has ("access to the Han empire"). Without any diplomacy rows a
# fresh faction has no relation entries, which the startpos writer does not handle.
param(
    [string]$FactionKey = 'sects_white_lotus',
    [long]$FactionId = 1900000000,
    [string]$DonorKey = '3k_main_faction_zhai_rong',
    [long]$DonorId = 1570674462
)

$ErrorActionPreference = 'Stop'
$db = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\raw_data\db'
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

$donorSuffix = ($DonorKey -replace '^3k_main_faction_', '')
$donorDealId = "3k_main_diplomatic_deal_start_pos_access_to_han_empire_faction_han_empire_faction_$donorSuffix"
$ourDealId = "3k_main_diplomatic_deal_start_pos_access_to_han_empire_faction_han_empire_faction_$FactionKey"

# 1. simple deal
$path = Join-Path $db 'start_pos_diplomacy_simple_deals.xml'
$x = Read-XmlFile $path
@($x.dataroot.start_pos_diplomacy_simple_deals | Where-Object { $_.recipient -eq "$FactionId" -or $_.id -eq $ourDealId }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$donor = $x.dataroot.start_pos_diplomacy_simple_deals | Where-Object { $_.id -eq $donorDealId } | Select-Object -First 1
if (-not $donor) { throw "donor simple deal $donorDealId not found" }
$rec = $donor.CloneNode($true)
$rec.record_uuid = New-Guid; $rec.record_timestamp = Now-Ms
$rec.record_key = $ourDealId; $rec.id = $ourDealId; $rec.recipient = "$FactionId"
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved simple deal"

# 2. deal
$path = Join-Path $db 'start_pos_diplomacy_deals.xml'
$x = Read-XmlFile $path
@($x.dataroot.start_pos_diplomacy_deals | Where-Object { $_.id -eq $ourDealId }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$donor = $x.dataroot.start_pos_diplomacy_deals | Where-Object { $_.id -eq $donorDealId } | Select-Object -First 1
if (-not $donor) { throw "donor deal $donorDealId not found" }
$rec = $donor.CloneNode($true)
$rec.record_uuid = New-Guid; $rec.record_timestamp = Now-Ms
$rec.record_key = $ourDealId; $rec.id = $ourDealId
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved deal"

# 3. deal ordering (record_key is the order number and must stay unique)
$path = Join-Path $db 'start_pos_diplomacy_deal_orderings.xml'
$x = Read-XmlFile $path
@($x.dataroot.start_pos_diplomacy_deal_orderings | Where-Object { $_.deal -eq $ourDealId }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$orders = @($x.dataroot.start_pos_diplomacy_deal_orderings | ForEach-Object { [int]$_.order })
$next = ($orders | Measure-Object -Maximum).Maximum + 1
$donor = $x.dataroot.start_pos_diplomacy_deal_orderings | Where-Object { $_.deal -eq $donorDealId } | Select-Object -First 1
if (-not $donor) { throw "donor ordering for $donorDealId not found" }
$rec = $donor.CloneNode($true)
$rec.record_uuid = New-Guid; $rec.record_timestamp = Now-Ms
$rec.record_key = "$next"; $rec.deal = $ourDealId; $rec.order = "$next"
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"saved deal ordering $next"
