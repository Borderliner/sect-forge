param([switch]$NoStart)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class BX {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int c);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
"@

$p = Get-Process bob.retail.x64 -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { throw 'bob not running' }
$h = $p.MainWindowHandle
[void][BX]::ShowWindow($h, 9)
[void][BX]::SetForegroundWindow($h)
Start-Sleep -Seconds 1

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$dlg = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond) | Select-Object -First 1

function Find-Button([string]$id) {
    $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $id)
    return $dlg.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c)
}

$selAll = Find-Button 'DATA_SELECTION_DIALOG.selectAllButton'
$selAll.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
'select all invoked'
Start-Sleep -Seconds 6

$start = Find-Button 'DATA_SELECTION_DIALOG.start_button'
"start enabled = $($start.Current.IsEnabled)"
"start name = $($start.Current.Name)"

# screenshot
Start-Sleep -Seconds 1
$r = New-Object BX+RECT
[void][BX]::GetWindowRect($h, [ref]$r)
$w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap($w, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
$dir = 'C:\Users\Touraj\Projects\Sects&Cults\screenshots'
New-Item -ItemType Directory -Force $dir | Out-Null
$out = Join-Path $dir 'bob_selected.png'
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
"saved $out"

if (-not $NoStart -and $start.Current.IsEnabled) {
    $start.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    'start invoked'
}
