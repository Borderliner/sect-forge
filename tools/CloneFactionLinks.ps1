param(
    [string]$DonorKey = '3k_main_faction_zhai_rong',
    [string]$FactionKey = 'sects_white_lotus',
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$db = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\raw_data\db'
$utf8 = New-Object System.Text.UTF8Encoding($false)

$tables = @(
    'building_chain_availabilities',
    'cai_personality_group_overrides',
    'campaign_group_member_criteria_factions',
    'campaign_map_attrition_faction_immunities',
    'campaign_stances_factions_junctions',
    'faction_banners',
    'faction_agent_permitted_subtypes',
    'faction_to_name_groups',
    'faction_uniform_colours',
    'unit_banners'
)

function New-Guid { '{' + [guid]::NewGuid().ToString() + '}' }
function Now-Ms { "$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())" }

function Append-Record([string]$path, [string]$recordXml) {
    $text = [IO.File]::ReadAllText($path)
    $idx = $text.LastIndexOf('</dataroot>')
    if ($idx -lt 0) { throw "dataroot end not found in $path" }
    $new = $text.Substring(0, $idx) + $recordXml + "`r`n" + $text.Substring($idx)
    [IO.File]::WriteAllText($path, $new, $utf8)
}

$summary = @()

foreach ($t in $tables) {
    $path = Join-Path $db "$t.xml"
    if (-not (Test-Path $path)) { $summary += "$t : MISSING"; continue }

    $text = [IO.File]::ReadAllText($path)
    $x = New-Object System.Xml.XmlDocument
    $x.LoadXml($text)

    # drop any previous clone of ours (idempotency)
    $existing = @($x.dataroot.ChildNodes | Where-Object {
            $_.NodeType -eq 'Element' -and $_.OuterXml -like "*$FactionKey*"
        })
    foreach ($e in $existing) { [void]$x.dataroot.RemoveChild($e) }
    if ($existing.Count -gt 0) { [IO.File]::WriteAllText($path, $x.OuterXml, $utf8) }

    # find donor rows: exact key match in record_key or any direct child text
    $donorRows = @($x.dataroot.ChildNodes | Where-Object {
            if ($_.NodeType -ne 'Element') { return $false }
            if ($_.GetAttribute('record_key') -eq $DonorKey) { return $true }
            foreach ($c in $_.ChildNodes) { if ($c.NodeType -eq 'Element' -and $c.InnerText -eq $DonorKey) { return $true } }
            return $false
        })

    $clones = 0
    foreach ($d in $donorRows) {
        $c = $d.CloneNode($true)
        if ($c.HasAttribute('record_uuid')) { $c.SetAttribute('record_uuid', "$(New-Guid)") }
        if ($c.HasAttribute('record_timestamp')) { $c.SetAttribute('record_timestamp', (Now-Ms)) }
        if ($c.HasAttribute('record_key')) {
            $c.SetAttribute('record_key', ($c.GetAttribute('record_key').Replace($DonorKey, $FactionKey)))
        }
        # replace key text in every element
        $all = $c.SelectNodes('.//*')
        foreach ($n in @($all) + @($c)) {
            if ($n.NodeType -eq 'Element' -and $n.ChildNodes.Count -eq 1 -and $n.FirstChild.NodeType -eq 'Text') {
                if ($n.InnerText -eq $DonorKey) { $n.InnerText = $FactionKey }
            }
        }
        if (-not $DryRun) { Append-Record $path $c.OuterXml }
        $clones++
    }
    $summary += "$t : $($donorRows.Count) donor rows -> $clones cloned"
}

$summary | ForEach-Object { $_ }
