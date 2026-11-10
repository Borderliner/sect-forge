param(
    [string]$DataPack = 'C:\Users\Touraj\Projects\Sects&Cults\build\SectsCults_white_lotus_Data.pack',
    [string]$StartposRoot = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS',
    [string]$ModName = 'SectsCults_white_lotus',
    [string]$FactionKey = 'sects_white_lotus'
)

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\rpfm_mcp.ps1"

function Find-Startpos([string]$root) {
    $hits = Get-ChildItem $root -Recurse -File -Filter 'startpos_*.esf' -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '3k_main_campaign_map' } |
        Sort-Object LastWriteTime -Descending
    return $hits
}

$startpos = Find-Startpos $StartposRoot
if (-not $startpos -or $startpos.Count -eq 0) {
    throw "No generated 3k_main_campaign_map startpos found under $StartposRoot."
}

# hard verification: the built startpos must contain the new faction
$verified = $false
foreach ($f in ($startpos | Select-Object -First 2)) {
    $bytes = [IO.File]::ReadAllBytes($f.FullName)
    $ascii = [Text.Encoding]::ASCII.GetString($bytes)
    if ($ascii.Contains($FactionKey)) { $verified = $true; "verified faction in $($f.Name)" }
}
if (-not $verified) {
    throw "The newest startpos does not contain $FactionKey; refusing to package an unchanged startpos."
}
"found startpos:"
$startpos | Select-Object LastWriteTime, Length, FullName | Format-Table -AutoSize | Out-String -Width 200

Start-Rpfm
Connect-Mcp
$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

$null = Mcp-Tool 'close_all_packs' @{}
$packInfo = (Mcp-Tool 'open_pack' @{ paths = @($DataPack) }).structuredContent
$pack = $packInfo.key
"opened data pack: $pack"

# add the generated startpos files to the mod under the campaign path
$existing = @($startpos | Where-Object { $_.Name -in @('startpos_historical.esf','startpos_romance.esf') })
if ($existing.Count -eq 0) { throw 'No historical/romance startpos found.' }
foreach ($f in $existing) {
    $null = Mcp-Tool 'add_files_from_disk' @{
        pack       = $pack
        paths      = @($f.FullName)
        folder     = 'campaigns/3k_main_campaign_map'
        pack_paths = @("campaigns/3k_main_campaign_map/$($f.Name)")
    }
    "added $($f.Name)"
}

# sect belief script
$lua = @"
-- Sects & Cults - $FactionKey
local TARGET = "$FactionKey"
local BUNDLE = "sects_white_lotus_belief"

cm:add_first_tick_callback(function()
    local ok, faction = pcall(function() return cm:get_local_faction() end)
    if ok and faction == TARGET then
        pcall(cm.apply_effect_bundle, cm, BUNDLE, TARGET, 0)
        if output then output("[sects] White Lotus founded: " .. tostring(faction)) end
    end
end)
"@
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($lua))
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = 'script/campaign/mod/sects_white_lotus.lua'; kind = @{ type = 'text'; format = 'Lua' } }
$null = Mcp-Tool 'write_file' @{ pack = $pack; path = 'script/campaign/mod/sects_white_lotus.lua'; contents = @{ kind = 'raw'; base64 = $b64 } }

$dir = Join-Path $env:APPDATA "com.spectra-mod-manager\mods\threekingdoms\$ModName"
$null = New-Item -ItemType Directory -Force $dir | Out-Null
$target = Join-Path $dir "$ModName.pack"
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $target; disable_uuid_regeneration = $true }
"saved $target"
