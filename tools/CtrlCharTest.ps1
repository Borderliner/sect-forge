# Control test: add a CLONE of Zhai Rong's leader as a second general of his OWN
# faction. Isolates "our character row is invalid" from "a new faction is invalid".
param(
    [long]$NewCharId = 1900000100,
    [string]$FactionId = '1570674462',
    [string]$DonorCharId = '1577879871',
    [string]$Mode = 'pool'   # pool | map | leader
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

$path = Join-Path $db 'start_pos_characters.xml'
$x = Read-XmlFile $path
$donor = $x.dataroot.start_pos_characters | Where-Object { $_.ID -eq $DonorCharId } | Select-Object -First 1
if (-not $donor) { throw 'donor character not found' }
@($x.dataroot.start_pos_characters | Where-Object { $_.ID -eq "$NewCharId" }) | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }

$rec = $donor.CloneNode($true)
$rec.record_uuid = '{' + [guid]::NewGuid().ToString() + '}'
$rec.record_timestamp = "$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())"
$rec.record_key = "$NewCharId"
$rec.ID = "$NewCharId"
$rec.faction = $FactionId
switch ($Mode) {
    'pool' {
        $rec.ministerial_position = ''
        $rec.is_in_generals_pool = '1'
        $rec.start_on_map = '0'
        $rec.startx = '1'; $rec.starty = '1'
    }
    'map' {
        $rec.ministerial_position = ''
        $rec.is_in_generals_pool = '0'
        $rec.start_on_map = '1'
        $rec.startx = '604'; $rec.starty = '436'
    }
    'leader' {
        $rec.ministerial_position = 'faction_leader'
        $rec.is_in_generals_pool = '0'
        $rec.start_on_map = '1'
        $rec.startx = '604'; $rec.starty = '436'
    }
}
[void]$x.dataroot.AppendChild($rec)
Save-XmlFile $x $path
"added character $NewCharId to faction $FactionId (mode=$Mode)"
