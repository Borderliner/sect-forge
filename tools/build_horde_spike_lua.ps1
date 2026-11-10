# Builds the Lua-only wander spike (no DB tables; the version that ran without crashing).
. "$PSScriptRoot\rpfm_mcp.ps1"

$ErrorActionPreference = 'Stop'
Start-Rpfm
Connect-Mcp

$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

$status = (Mcp-Tool 'session_status' @{}).structuredContent
foreach ($p in $status.packs) {
    if ($p.name -eq 'SectsCults_HordeSpike.pack') { $null = Mcp-Tool 'close_pack' @{ pack = $p.key } }
}

$pack = (Mcp-Tool 'new_pack' @{ name = 'SectsCults_HordeSpike.pack' }).structuredContent.key
"pack: $pack"

$lua = Get-Content (Join-Path $PSScriptRoot '..\src\script\sects_horde_spike.lua') -Raw
$b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($lua))
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = 'script/campaign/mod/sects_horde_spike.lua'; kind = @{ type = 'text'; format = 'Lua' } }
$null = Mcp-Tool 'write_file' @{ pack = $pack; path = 'script/campaign/mod/sects_horde_spike.lua'; contents = @{ kind = 'raw'; base64 = $b64 } }

$outDir = Join-Path $PSScriptRoot '..\build'
$target = (Resolve-Path $outDir).Path + '\SectsCults_HordeSpike.pack'
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $target; disable_uuid_regeneration = $true }
"saved $target"
