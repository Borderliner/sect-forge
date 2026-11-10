Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class BW {
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int w,int hgt,bool r);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int c);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
"@

$exe = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\binaries\bob.retail.x64.exe'
Get-Process bob.retail.x64, bob.modder.x64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe)
Start-Sleep -Seconds 40

$p = Get-Process bob.retail.x64 -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { 'bob not running'; exit 1 }
$h = $p.MainWindowHandle
[void][BW]::ShowWindow($h, 9)
[void][BW]::MoveWindow($h, 30, 30, 1250, 800, $true)
[void][BW]::SetForegroundWindow($h)
Start-Sleep -Seconds 2

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$dlg = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond) | Select-Object -First 1
if (-not $dlg) { 'no window element'; exit 1 }

'### buttons'
$btns = $dlg.FindAll([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)))
foreach ($b in $btns) {
    '{0} | id={1} | enabled={2}' -f $b.Current.Name, $b.Current.AutomationId, $b.Current.IsEnabled
}
'### checkboxes'
$cbs = $dlg.FindAll([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::CheckBox)))
'{0} checkboxes' -f $cbs.Count
