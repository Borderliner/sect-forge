Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WB {
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int w,int hgt,bool r);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int c);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
"@

$exe = 'C:\Program Files (x86)\Steam\steamapps\common\Total War THREE KINGDOMS\assembly_kit\binaries\bob.retail.x64.exe'
Get-Process bob.retail.x64, bob.modder.x64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe)
Start-Sleep -Seconds 32

$p = Get-Process bob.retail.x64 -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { exit }
$h = $p.MainWindowHandle
[void][WB]::ShowWindow($h, 9)
[void][WB]::MoveWindow($h, 30, 30, 1250, 800, $true)
[void][WB]::SetForegroundWindow($h)

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$dlg = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond) | Select-Object -First 1
$c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'DATA_SELECTION_DIALOG.configurationEditorButton')
$btn = $dlg.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c)
if ($btn) { $btn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke() }
Start-Sleep -Seconds 2
$ed = $dlg.FindFirst([System.Windows.Automation.TreeScope]::Descendants, (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'DATA_SELECTION_DIALOG.CONFIGURATION_EDITOR_DIALOG')))
if ($ed -and $ed.Current.NativeWindowHandle -ne 0) {
    [void][WB]::MoveWindow([IntPtr]$ed.Current.NativeWindowHandle, 180, 140, 900, 600, $true)
    [void][WB]::SetForegroundWindow([IntPtr]$ed.Current.NativeWindowHandle)
}
