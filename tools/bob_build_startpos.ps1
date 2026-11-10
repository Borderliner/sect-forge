param(
    [string]$BobExe = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\binaries\bob.retail.x64.exe',
    [int]$TimeoutMinutes = 15
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$bins = Split-Path $BobExe
$null = New-Item -ItemType Directory -Force (Join-Path $bins 'BOB')

Get-Process bob.retail.x64, bob.modder.x64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$p = Start-Process -FilePath $BobExe -WorkingDirectory $bins -PassThru
Start-Sleep -Seconds 35

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$dlg = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond) | Select-Object -First 1
if (-not $dlg) { throw 'Bob main window not found.' }

function Find-ById($parent, $id) {
    $c = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::AutomationIdProperty, $id)
    return $parent.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c)
}

# Mark the campaign inputs/outputs, exactly as the successful manual-equivalent run did.
foreach ($pair in @(
    @('DATA_SELECTION_DIALOG.splitter.raw_data_files', @('db', 'EmpireDesignData')),
    @('DATA_SELECTION_DIALOG.splitter.working_data_files', @('campaigns', 'campaign_maps'))
)) {
    $tree = Find-ById $dlg $pair[0]
    if (-not $tree) { continue }
    $items = $tree.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($name in $pair[1]) {
        $it = $items | Where-Object { $_.Current.Name -eq $name } | Select-Object -First 1
        if ($it) {
            try { $it.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); "invoked $name" } catch { }
        }
    }
}

Start-Sleep -Seconds 3
$start = Find-ById $dlg 'DATA_SELECTION_DIALOG.start_button'
if (-not $start) { throw 'Start button not found.' }
$start.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
"start clicked"

$sw = [Diagnostics.Stopwatch]::StartNew()
$gameSeen = $false
while ($sw.Elapsed.TotalMinutes -lt $TimeoutMinutes) {
    Start-Sleep -Seconds 5
    if ((Get-Process Three_Kingdoms -ErrorAction SilentlyContinue) -and -not $gameSeen) {
        $gameSeen = $true
        "game launched at $([int]$sw.Elapsed.TotalSeconds)s"
    }
    if (-not (Get-Process -Id $p.Id -ErrorAction SilentlyContinue)) {
        "bob exited at $([int]$sw.Elapsed.TotalSeconds)s"
        break
    }
}
"gameSeen=$gameSeen"

$g = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS'
$campaignDir = Join-Path $g 'data\campaigns\3k_main_campaign_map'
$found = Get-ChildItem $campaignDir -File -Filter 'startpos_*.esf' -ErrorAction SilentlyContinue |
    Where-Object { $_.LastWriteTime -gt (Get-Date).AddMinutes(-$TimeoutMinutes) }
if ($found) {
    "generated 190 startpos:"
    $found | Select-Object LastWriteTime, Length, FullName
} else {
    "no new 190 startpos found in $campaignDir"
}
