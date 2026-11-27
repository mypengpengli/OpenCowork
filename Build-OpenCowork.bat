@echo off
setlocal
set "ROOT=%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%ROOT%scripts\launch-windows.ps1" -BuildOnly
exit /b %ERRORLEVEL%
