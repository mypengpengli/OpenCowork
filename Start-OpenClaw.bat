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

where cargo >nul 2>nul
if errorlevel 1 (
  echo [OpenClaw] cargo was not found.
  echo [OpenClaw] Install the Rust toolchain first: rustup + cargo.
  echo [OpenClaw] See README ^> Windows Quick Start.
  pause
  popd >nul
  exit /b 1
)

where rustc >nul 2>nul
if errorlevel 1 (
  echo [OpenClaw] rustc was not found.
  echo [OpenClaw] Install the Rust toolchain first: rustup + cargo.
  echo [OpenClaw] See README ^> Windows Quick Start.
  pause
  popd >nul
  exit /b 1
)

echo [OpenClaw] Building local shell...
cargo build --target-dir "%TARGET_DIR%" -p opencowork-shell
if errorlevel 1 goto :fallback_web

echo [OpenClaw] Building desktop host...
cargo build --target-dir "%TARGET_DIR%" -p opencowork-desktop
if errorlevel 1 goto :fallback_web

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
echo [OpenClaw] Building web shell...
cargo build --target-dir "%TARGET_DIR%" -p opencowork-shell
if errorlevel 1 (
  echo [OpenClaw] Web shell build failed.
  echo [OpenClaw] Make sure Rust, MSVC Build Tools, and WebView2 Runtime are installed.
  pause
  popd >nul
  exit /b 1
)

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
