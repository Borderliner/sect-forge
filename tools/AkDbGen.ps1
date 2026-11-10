param(
    [string]$SectName = 'Way of the White Lotus',
    [string]$FactionKey = 'sects_white_lotus',
    [long]$ForenameId = 1900000001,
    [long]$SurnameId = 1900000002,
    [string]$Forename = 'Wei',
    [string]$Surname = 'Yun',
    [bool]$IsMale = $true,
    [string]$ArtSet = '3k_main_art_set_generic_male_48_general_wood',
    [string]$Subtype = '3k_general_wood',
    [string]$Voiceover = 'vo_actor_group_generic_any_male_wood_general',
    [string]$PrimaryColour = '6A0DAD',
    [string]$SecondaryColour = 'E0C060'
)

$ErrorActionPreference = 'Stop'
$db = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\raw_data\db'
$slug = ($FactionKey -replace '^sects_', '')
$templateKey = "sects_${slug}_leader"

function New-Guid { '{' + [guid]::NewGuid().ToString() + '}' }
function Now-Ms { "$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())" }
function Set-Text($el, [string]$text) {
    if ($el) { $el.InnerText = $text; try { $el.SetAttribute('last_approved_text', $text) } catch { } }
}
function Hex-Part([string]$hex, [int]$start) { return [Convert]::ToInt32($hex.Substring($start, 2), 16) }

# --- faction ----------------------------------------------------------------
$path = Join-Path $db 'factions.xml'
$x = [xml](Get-Content $path -Raw)
$x.dataroot.factions | Where-Object { $_.key -eq $FactionKey } | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$donor = $x.dataroot.factions | Where-Object { $_.key -eq '3k_main_faction_zhai_rong' } | Select-Object -First 1
if (-not $donor) { throw 'Faction donor not found in AK factions.xml.' }
$rec = $donor.CloneNode($true)
$rec.record_uuid = "$(New-Guid)"
$rec.record_timestamp = Now-Ms
$rec.record_key = $FactionKey
$rec.key = $FactionKey
Set-Text $rec.screen_name $SectName
Set-Text $rec.screen_adjective $SectName
Set-Text $rec.screen_name_when_rebels "$SectName Renegades"
$rec.category = 'playable'
$rec.feature_forest = 'LOCKED_STANDARD'
$rec.index = '199999999'
$rec.primary_colour_r = "$(Hex-Part $PrimaryColour 0)"
$rec.primary_colour_g = "$(Hex-Part $PrimaryColour 2)"
$rec.primary_colour_b = "$(Hex-Part $PrimaryColour 4)"
$rec.secondary_colour_r = "$(Hex-Part $SecondaryColour 0)"
$rec.secondary_colour_g = "$(Hex-Part $SecondaryColour 2)"
$rec.secondary_colour_b = "$(Hex-Part $SecondaryColour 4)"
$rec.uniform_colour_r = "$(Hex-Part $PrimaryColour 0)"
$rec.uniform_colour_g = "$(Hex-Part $PrimaryColour 2)"
$rec.uniform_colour_b = "$(Hex-Part $PrimaryColour 4)"
[void]$x.dataroot.AppendChild($rec)
$x.Save($path)
"saved faction $FactionKey"

# --- names ------------------------------------------------------------------
$path = Join-Path $db 'names.xml'
$x = [xml](Get-Content $path -Raw)
foreach ($pair in @(@($ForenameId, 'forename', 'm', $Forename), @($SurnameId, 'family_name', 'b', $Surname))) {
    $id = "$($pair[0])"
    $x.dataroot.names | Where-Object { $_.id -eq $id } | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
    $donorName = if ($pair[1] -eq 'forename') {
        $x.dataroot.names | Where-Object { $_.id -eq '309319222' } | Select-Object -First 1
    } else {
        $x.dataroot.names | Where-Object { $_.id -eq '654857049' } | Select-Object -First 1
    }
    $n = $donorName.CloneNode($true)
    $n.record_uuid = "$(New-Guid)"
    $n.record_timestamp = Now-Ms
    $n.record_key = $id
    $n.id = $id
    $n.type = $pair[1]
    $n.gender = $pair[2]
    Set-Text $n.name $pair[3]
    [void]$x.dataroot.AppendChild($n)
}
$x.Save($path)
"saved names"

# --- template ---------------------------------------------------------------
$path = Join-Path $db 'character_generation_templates.xml'
$x = [xml](Get-Content $path -Raw)
$x.dataroot.character_generation_templates | Where-Object { $_.key -eq $templateKey } | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$donorT = $x.dataroot.character_generation_templates | Where-Object { $_.key -eq '3k_main_template_generated_ze_bu_hero_wood' } | Select-Object -First 1
$t = $donorT.CloneNode($true)
$t.record_uuid = "$(New-Guid)"
$t.record_timestamp = Now-Ms
$t.record_key = $templateKey
$t.key = $templateKey
$t.forename = "$ForenameId"
$t.family_name = "$SurnameId"
$t.clan_name = ''
$t.other_name = ''
if ($IsMale) { $t.is_male = '1' } else { $t.is_male = '0' }$t.unique = '1'
$t.art_set_override = $ArtSet
$t.subtype = $Subtype
$t.voiceover_actor = $Voiceover
[void]$x.dataroot.AppendChild($t)
$x.Save($path)
"saved template $templateKey"

# --- template game-mode details (retinue etc.) ------------------------------
$path = Join-Path $db 'character_generation_template_game_mode_details.xml'
$x = [xml](Get-Content $path -Raw)
$x.dataroot.character_generation_template_game_mode_details | Where-Object { $_.character_generation_template -eq $templateKey } | ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$donorRows = $x.dataroot.character_generation_template_game_mode_details | Where-Object { $_.character_generation_template -eq '3k_main_template_generated_ze_bu_hero_wood' }
foreach ($d in $donorRows) {
    $g = $d.CloneNode($true)
    $g.record_uuid = "$(New-Guid)"
    $g.record_timestamp = Now-Ms
    $g.record_key = "$templateKey" + $g.game_mode
    $g.character_generation_template = $templateKey
    [void]$x.dataroot.AppendChild($g)
}
$x.Save($path)
"saved game-mode details"

