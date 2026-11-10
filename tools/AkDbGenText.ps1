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
    [string]$SecondaryColour = 'E0C060',
    [string]$Category = 'playable',
    [long]$FactionIndex = 167374819,
    [string]$DonorKey = '3k_main_faction_zhai_rong'
)

$ErrorActionPreference = 'Stop'
$db = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\raw_data\db'
$bak = 'C:\Users\Touraj\Projects\Sects&Cults\backup_ak_startpos'
$slug = ($FactionKey -replace '^sects_', '')
$templateKey = "sects_${slug}_leader"
$utf8 = New-Object System.Text.UTF8Encoding($false)

function New-Guid { '{' + [guid]::NewGuid().ToString() + '}' }
function Now-Ms { "$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())" }
function Set-Text($el, [string]$text) {
    if ($el) { $el.InnerText = $text; try { $el.SetAttribute('last_approved_text', $text) } catch { } }
}
function Hex-Part([string]$hex, [int]$start) { return [Convert]::ToInt32($hex.Substring($start, 2), 16) }
function Append-Record([string]$path, [string]$recordXml) {
    $text = [IO.File]::ReadAllText($path)
    $idx = $text.LastIndexOf('</dataroot>')
    if ($idx -lt 0) { throw "dataroot end not found in $path" }
    $new = $text.Substring(0, $idx) + $recordXml + "`r`n" + $text.Substring($idx)
    [IO.File]::WriteAllText($path, $new, $utf8)
}

# Restore clean originals first (game_mode_details has no backup; handled below)
foreach ($n in @('factions.xml','names.xml','character_generation_templates.xml')) {
    Copy-Item (Join-Path $bak $n) (Join-Path $db $n) -Force
}

# --- faction ----------------------------------------------------------------
$x = [xml](Get-Content (Join-Path $bak 'factions.xml') -Raw)
$donor = $x.dataroot.factions | Where-Object { $_.key -eq $DonorKey } | Select-Object -First 1
if (-not $donor) { throw "faction donor $DonorKey not found" }
$rec = $donor.CloneNode($true)
$rec.record_uuid = "$(New-Guid)"; $rec.record_timestamp = Now-Ms; $rec.record_key = $FactionKey
$rec.key = $FactionKey
Set-Text $rec.screen_name $SectName
Set-Text $rec.screen_adjective $SectName
Set-Text $rec.screen_name_when_rebels "$SectName Renegades"
$rec.category = $Category
$rec.feature_forest = 'LOCKED_STANDARD'
$rec.index = "$FactionIndex"
$rec.primary_colour_r = "$(Hex-Part $PrimaryColour 0)"
$rec.primary_colour_g = "$(Hex-Part $PrimaryColour 2)"
$rec.primary_colour_b = "$(Hex-Part $PrimaryColour 4)"
$rec.secondary_colour_r = "$(Hex-Part $SecondaryColour 0)"
$rec.secondary_colour_g = "$(Hex-Part $SecondaryColour 2)"
$rec.secondary_colour_b = "$(Hex-Part $SecondaryColour 4)"
$rec.uniform_colour_r = "$(Hex-Part $PrimaryColour 0)"
$rec.uniform_colour_g = "$(Hex-Part $PrimaryColour 2)"
$rec.uniform_colour_b = "$(Hex-Part $PrimaryColour 4)"
Append-Record (Join-Path $db 'factions.xml') $rec.OuterXml
"saved faction"

# --- names ------------------------------------------------------------------
$x = [xml](Get-Content (Join-Path $bak 'names.xml') -Raw)
$records = @()
foreach ($pair in @(@($ForenameId, 'forename', 'm', $Forename, '309319222'), @($SurnameId, 'family_name', 'b', $Surname, '654857049'))) {
    $donorN = $x.dataroot.names | Where-Object { $_.id -eq $pair[4] } | Select-Object -First 1
    $n = $donorN.CloneNode($true)
    $n.record_uuid = "$(New-Guid)"; $n.record_timestamp = Now-Ms; $n.record_key = "$($pair[0])"
    $n.id = "$($pair[0])"; $n.type = $pair[1]; $n.gender = $pair[2]
    Set-Text $n.name $pair[3]
    $records += $n.OuterXml
}
foreach ($r in $records) { Append-Record (Join-Path $db 'names.xml') $r }
"saved names"

# --- template ---------------------------------------------------------------
# A startpos character must use a NON-unique template (unique=0); unique
# templates can only appear once in the game, which crashes startpos generation.
$x = [xml](Get-Content (Join-Path $bak 'character_generation_templates.xml') -Raw)
$startposCharacters = [IO.File]::ReadAllText((Join-Path $bak 'start_pos_characters.xml'))
$candidates = @($x.dataroot.character_generation_templates | Where-Object { $_.unique -eq '0' -and $_.subtype -eq $Subtype })
if ($candidates.Count -eq 0) { throw "No non-unique character template found for subtype $Subtype" }
$donorT = @($candidates | Where-Object { $startposCharacters.Contains('>' + $_.key + '<') } | Select-Object -First 1)
if (-not $donorT) { $donorT = @($candidates | Where-Object { $_.art_set_override -ne '' } | Select-Object -First 1) }
if (-not $donorT) { $donorT = @($candidates | Select-Object -First 1) }
$donorT = $donorT[0]
$donorTemplateKey = $donorT.key
$t = $donorT.CloneNode($true)
$t.record_uuid = "$(New-Guid)"; $t.record_timestamp = Now-Ms; $t.record_key = $templateKey
$t.key = $templateKey
$t.forename = "$ForenameId"; $t.family_name = "$SurnameId"
$t.clan_name = ''; $t.other_name = ''
if ($IsMale) { $t.is_male = '1' } else { $t.is_male = '0' }
$t.unique = '0'; $t.can_be_born = '0'
$t.art_set_override = $ArtSet; $t.subtype = $Subtype; $t.voiceover_actor = $Voiceover
Append-Record (Join-Path $db 'character_generation_templates.xml') $t.OuterXml
"saved template (based on $donorTemplateKey)"

# --- game-mode details ------------------------------------------------------
$gmPath = Join-Path $db 'character_generation_template_game_mode_details.xml'
$x = [xml](Get-Content $gmPath -Raw)
$x.dataroot.character_generation_template_game_mode_details |
    Where-Object { $_.character_generation_template -eq $templateKey } |
    ForEach-Object { [void]$x.dataroot.RemoveChild($_) }
$donorRows = $x.dataroot.character_generation_template_game_mode_details |
    Where-Object { $_.character_generation_template -eq $donorTemplateKey }
$recordXmls = @()
foreach ($d in $donorRows) {
    $g = $d.CloneNode($true)
    $g.record_uuid = "$(New-Guid)"; $g.record_timestamp = Now-Ms
    $g.record_key = "$templateKey" + $g.game_mode
    $g.character_generation_template = $templateKey
    $recordXmls += $g.OuterXml
}
# save the cleaned document without BOM, then append the new records as text
$settings = New-Object System.Xml.XmlWriterSettings
$settings.Encoding = $utf8
$settings.Indent = $true
$writer = [System.Xml.XmlWriter]::Create($gmPath, $settings)
$x.Save($writer); $writer.Close()
foreach ($r in $recordXmls) { Append-Record $gmPath $r }
"saved $($recordXmls.Count) game-mode rows"
