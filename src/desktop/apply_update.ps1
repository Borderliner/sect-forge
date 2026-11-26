$ErrorActionPreference = 'Stop'
$installed = $env:SECTFORGE_UPDATE_OLD
$download = $env:SECTFORGE_UPDATE_NEW
$staging = [IO.Path]::GetDirectoryName($download)
$previous = Join-Path $staging 'previous.exe'

try {
    $running = Get-Process -Id ([int]$env:SECTFORGE_UPDATE_PID) -ErrorAction SilentlyContinue
    if ($running -and $running.Path -eq $installed) {
        $running.WaitForExit()
    }
    [IO.File]::Replace($download, $installed, $previous)
    Remove-Item -LiteralPath $previous -ErrorAction SilentlyContinue
} catch {
    $message = $_.Exception.Message
    if (Test-Path -LiteralPath $previous) {
        $message += "`nPrevious executable: $previous"
    }
    Add-Type -AssemblyName System.Windows.Forms
    [void][System.Windows.Forms.MessageBox]::Show($message, 'Sect Forge update failed')
} finally {
    Remove-Item -LiteralPath $download -ErrorAction SilentlyContinue
    if (-not (Test-Path -LiteralPath $previous)) {
        Remove-Item -LiteralPath $staging -ErrorAction SilentlyContinue
    }
}
