@echo off
setlocal

set "ROOT=%~dp0"
set "TARGET_DIR=%LOCALAPPDATA%\OpenClaw\target"

pushd "%ROOT%" >nul 2>nul
if errorlevel 1 exit /b 1

where cargo >nul 2>nul
if errorlevel 1 (
  echo [OpenCowork] Install the Rust toolchain and MSVC Build Tools. See README.
  popd >nul
  exit /b 1
)

echo [OpenCowork] Building shell and desktop host...
call cargo build --locked --target-dir "%TARGET_DIR%" -p opencowork-shell -p opencowork-desktop
set "BUILD_RESULT=%ERRORLEVEL%"
if "%BUILD_RESULT%"=="0" >"%TARGET_DIR%\build-root.txt" <nul set /p "=%ROOT%"
popd >nul
exit /b %BUILD_RESULT%
