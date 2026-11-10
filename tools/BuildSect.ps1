param(
    [Parameter(Mandatory)]
    [string]$ConfigPath
)

$ErrorActionPreference = 'Stop'
$python = Get-Command python.exe -ErrorAction SilentlyContinue
if (-not $python) { throw 'Sect Forge requires Python 3.10 or newer.' }
& $python.Source -B (Join-Path $PSScriptRoot 'sectforge.py') $ConfigPath
exit $LASTEXITCODE
