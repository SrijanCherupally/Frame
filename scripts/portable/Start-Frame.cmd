@echo off
setlocal
if not exist "%~dp0ffmpeg.exe" goto missing
if not exist "%~dp0ffprobe.exe" goto missing
if not exist "%~dp0ffplay.exe" goto missing
start "" "%~dp0Frame.exe"
exit /b 0
:missing
echo Please extract ALL files from the full Frame ZIP into one folder.
echo The offline package includes Frame.exe, ffmpeg.exe, ffprobe.exe and ffplay.exe.
pause
exit /b 1
