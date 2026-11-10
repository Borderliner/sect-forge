param(
    [string]$FactionKey = 'sects_white_lotus',
    [long]$FactionId = 1900000000,
    [long]$LeaderId = 1900000100,
    [string]$Root = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS'
)

$ErrorActionPreference = 'Stop'

$files = Get-ChildItem $Root -Recurse -File -Filter 'startpos_*.esf' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '3k_main_campaign_map' } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 2

if (-not $files) { throw 'No 3k_main_campaign_map startpos found.' }

$ok = $false
foreach ($f in $files) {
    $bytes = [IO.File]::ReadAllBytes($f.FullName)
    $ascii = [Text.Encoding]::ASCII.GetString($bytes)
    $hasKey = $ascii.Contains($FactionKey)
    $hasId = $ascii.Contains("$FactionId")
    $hasLeader = $ascii.Contains("$LeaderId")
    "{0}`n  size={1}`n  factionKey={2} factionId={3} leaderId={4}" -f $f.FullName, $bytes.Length, $hasKey, $hasId, $hasLeader
    if ($hasKey) { $ok = $true }
}

if (-not $ok) {
    throw "Generated startpos does not contain $FactionKey. The faction rows were not picked up by the build."
}
"startpos verification passed"
