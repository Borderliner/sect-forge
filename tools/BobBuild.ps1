param(
    [switch]$RestoreStartpos,
    [switch]$RestoreDb,
    [int]$TimeoutMinutes = 25
)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class BY {
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int w,int hgt,bool r);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int c);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
"@

$game = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS'
$db = Join-Path $game 'assembly_kit\raw_data\db'
$bak = 'C:\Users\Touraj\Projects\Sects&Cults\backup_ak_startpos'

if ($RestoreStartpos) {
    Copy-Item (Join-Path $bak 'start_pos_*.xml') $db -Force
    'restored start_pos_* from backup'
}
if ($RestoreDb) {
    Copy-Item (Join-Path $bak 'factions.xml'), (Join-Path $bak 'names.xml'), (Join-Path $bak 'character_generation_templates.xml') $db -Force
    'restored factions/names/templates from backup'
}

Get-Process bob.retail.x64, bob.modder.x64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
$exe = Join-Path $game 'assembly_kit\binaries\bob.retail.x64.exe'
Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe)
Start-Sleep -Seconds 40

$p = Get-Process bob.retail.x64 -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { throw 'bob did not start' }
$h = $p.MainWindowHandle
[void][BY]::ShowWindow($h, 9)
[void][BY]::MoveWindow($h, 30, 30, 1250, 800, $true)
[void][BY]::SetForegroundWindow($h)
Start-Sleep -Seconds 2

$root = [System.Windows.Automation.AutomationElement]::RootElement

function Find-Button([string]$id) {
    $proc = Get-Process bob.retail.x64 -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $proc) { return $null }
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $proc.Id)
    $dlg = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond) | Select-Object -First 1
    if (-not $dlg) { return $null }
    $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $id)
    return $dlg.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c)
}

# Bob can take a while (or reopen) before the selection dialog is ready: retry.
$selAll = $null
for ($i = 0; $i -lt 30; $i++) {
    $selAll = Find-Button 'DATA_SELECTION_DIALOG.selectAllButton'
    if ($selAll) { break }
    Start-Sleep -Seconds 3
}
if (-not $selAll) { throw 'select all button not found' }
$selAll.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
Start-Sleep -Seconds 6
$start = Find-Button 'DATA_SELECTION_DIALOG.start_button'
if (-not $start) { throw 'start button not found' }
if (-not $start.Current.IsEnabled) { throw 'start button disabled after select all' }
$start.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
'build started'

$deadline = (Get-Date).AddMinutes($TimeoutMinutes)
$sawGame = $false
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 10
    $b = Get-Process bob.retail.x64 -ErrorAction SilentlyContinue | Select-Object -First 1
    $g = Get-Process Three_Kingdoms -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($g) { $sawGame = $true }
    if ($b -and $b.MainWindowTitle -notmatch 'Processing') { 'build finished (bob idle)'; break }
    if (-not $b) { 'bob exited'; break }
}
"game process seen: $sawGame"

'### 190 startpos'
Get-ChildItem (Join-Path $game 'data\campaigns\3k_main_campaign_map') -File -ErrorAction SilentlyContinue | Select-Object LastWriteTime, Length, Name | Format-Table -AutoSize | Out-String -Width 160
'### bob error'
Get-Content (Join-Path $game 'assembly_kit\binaries\bob_error.log') -Raw -ErrorAction SilentlyContinue
'### latest crash dumps'
Get-ChildItem "$env:LOCALAPPDATA\CrashDumps" -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 3 LastWriteTime, Name | Format-Table -AutoSize | Out-String -Width 120
