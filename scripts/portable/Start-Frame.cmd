@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Setup-Media-and-Start.ps1"
if errorlevel 1 (
  echo.
  echo Frame could not start. Read the message above, then try again.
  pause
)
