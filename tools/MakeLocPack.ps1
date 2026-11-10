param(
    [string]$FactionKey = 'sects_white_lotus',
    [string]$SepKey = 'sects_white_lotus_separatists',
    [string]$SectName = 'Way of the White Lotus',
    [string]$Forename = 'Wei',
    [string]$Surname = 'Yun',
    [long]$ForenameId = 1900000001,
    [long]$SurnameId = 1900000002,
    [long]$FactionId = 1900000000,
    [long]$SepId = 1900000050,
    [switch]$InstallToGame
)

# Builds a loc-only pack holding every localisation key our new faction needs.
# Missing loc for a faction name is enough to crash the startpos build, so this
# pack is dropped into the game's data folder as a Movie pack while building.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\rpfm_mcp.ps1"
Start-Rpfm
Connect-Mcp
$null = Mcp-Tool 'set_game' @{ game = 'three_kingdoms' }

$packName = 'sects_white_lotus_loc.pack'
$status = (Mcp-Tool 'session_status' @{}).structuredContent
foreach ($p in $status.packs) { if ($p.name -eq $packName) { $null = Mcp-Tool 'close_pack' @{ pack = $p.key } } }
$pack = (Mcp-Tool 'new_pack' @{ name = $packName }).structuredContent.key
"pack: $pack"

$locPath = 'text/db/sects_white_lotus__.loc'
$null = Mcp-Tool 'create_file' @{ pack = $pack; path = $locPath; kind = @{ type = 'loc' } }

$rows = @(
    @{ key = "factions_screen_name_$FactionKey"; text = $SectName; tooltip = $false },
    @{ key = "factions_screen_adjective_$FactionKey"; text = $SectName; tooltip = $false },
    @{ key = "factions_screen_name_when_rebels_$FactionKey"; text = "$SectName Renegades"; tooltip = $false },
    @{ key = "factions_screen_name_$SepKey"; text = "$SectName Separatists"; tooltip = $false },
    @{ key = "factions_screen_adjective_$SepKey"; text = "$SectName Separatists"; tooltip = $false },
    @{ key = "factions_screen_name_when_rebels_$SepKey"; text = "$SectName Separatists"; tooltip = $false },
    @{ key = "names_name_$ForenameId"; text = $Forename; tooltip = $false },
    @{ key = "names_name_$SurnameId"; text = $Surname; tooltip = $false },
    @{ key = "start_pos_factions_description_$FactionId"; text = 'placeholder'; tooltip = $false },
    @{ key = "start_pos_factions_long_description_$FactionId"; text = 'placeholder'; tooltip = $false },
    @{ key = "start_pos_factions_description_$SepId"; text = 'placeholder'; tooltip = $false },
    @{ key = "start_pos_factions_long_description_$SepId"; text = 'placeholder'; tooltip = $false }
) | ForEach-Object { @{ op = 'insert'; values = $_ } }

$r = Mcp-Tool 'edit_table' @{ pack = $pack; path = $locPath; edits = $rows }
if ($r.isError) { throw ("loc write failed: " + ($r.content.text -join ' ')) }
"wrote $($rows.Count) loc rows"

if ($InstallToGame) {
    $target = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\data\sects_white_lotus_loc.pack'
} else {
    $outDir = Join-Path $PSScriptRoot '..\build'
    $null = New-Item -ItemType Directory -Force $outDir | Out-Null
    $target = (Resolve-Path $outDir).Path + "\$packName"
}
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $target; disable_uuid_regeneration = $true }
try { $null = Mcp-Tool 'update_pack' @{ pack = $pack; pack_type = 'Movie' } } catch { "update_pack(pack_type) failed: $($_.Exception.Message)" }
$null = Mcp-Tool 'save_pack' @{ pack = $pack; path = $target; disable_uuid_regeneration = $true }

"saved $target"
