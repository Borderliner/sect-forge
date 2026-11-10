$ErrorActionPreference = 'Stop'
$python = Get-Command pythonw.exe -ErrorAction SilentlyContinue
if (-not $python) { throw 'Sect Forge requires Python 3.10 or newer with Tkinter.' }
& $python.Source -B (Join-Path $PSScriptRoot 'sectforge_app.py')
