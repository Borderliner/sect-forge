@echo off
where pythonw.exe >nul 2>nul
if errorlevel 1 (
    echo Sect Forge requires Python 3.10 or newer with Tkinter.
    pause
    exit /b 1
)
start "" pythonw.exe -B "%~dp0tools\sectforge_app.py"
