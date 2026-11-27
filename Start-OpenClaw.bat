@echo off
setlocal
set "ROOT=%~dp0"
if /I "%~1"=="--build" (
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%ROOT%scripts\launch-windows.ps1" -Rebuild
) else (
  powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%ROOT%scripts\launch-windows.ps1"
)
if errorlevel 1 (
  pause
  exit /b 1
)
exit /b 0
