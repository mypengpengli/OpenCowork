@echo off
setlocal

set "ROOT=%~dp0"
set "TARGET_DIR=%LOCALAPPDATA%\OpenClaw\target"
set "DESKTOP_EXE=%TARGET_DIR%\debug\opencowork-desktop.exe"
set "SHELL_EXE=%TARGET_DIR%\debug\opencowork-shell.exe"
set "SHELL_URL=http://127.0.0.1:33211/"

pushd "%ROOT%" >nul 2>nul
if errorlevel 1 (
  echo [OpenClaw] Failed to enter project directory: %ROOT%
  pause
  exit /b 1
)

rem Daily startup runs the existing binaries. Rebuild explicitly after source changes.
if /I "%~1"=="--build" goto :build
if not exist "%SHELL_EXE%" goto :build
if not exist "%TARGET_DIR%\build-root.txt" goto :build
set /p BUILT_ROOT=<"%TARGET_DIR%\build-root.txt"
if /I not "%BUILT_ROOT%"=="%ROOT%" goto :build
if not exist "%DESKTOP_EXE%" goto :fallback_web
goto :launch

:build
call "%ROOT%Build-OpenCowork.bat"
if errorlevel 1 (
  echo [OpenClaw] Build failed. See the output above.
  pause
  popd >nul
  exit /b 1
)

:launch

if not exist "%DESKTOP_EXE%" (
  echo [OpenClaw] Desktop executable was not found. Falling back to web shell...
  goto :fallback_web
)

if not exist "%SHELL_EXE%" (
  echo [OpenClaw] Shell executable was not found. Falling back to web shell...
  goto :fallback_web
)

echo [OpenClaw] Launching desktop app...
start "" "%DESKTOP_EXE%"
popd >nul
exit /b 0

:fallback_web
echo [OpenClaw] Desktop host is unavailable. Starting web shell instead...
if not exist "%SHELL_EXE%" (
  echo [OpenClaw] Web shell executable was not found: %SHELL_EXE%
  pause
  popd >nul
  exit /b 1
)

echo [OpenClaw] Launching web shell...
start "" "%SHELL_EXE%"
timeout /t 2 /nobreak >nul
start "" "%SHELL_URL%"
popd >nul
exit /b 0
