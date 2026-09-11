param([ValidateSet('release','debug')][string]$Profile = 'release')
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$buildCache = Join-Path $env:LOCALAPPDATA 'OpenClaw\target'
Push-Location $projectRoot
try {
    $arguments = @('build','--locked','--target-dir',$buildCache,'-p','opencowork-shell','-p','opencowork-desktop')
    if ($Profile -eq 'release') { $arguments += '--release' }
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed' }
    $version = ((Get-Content Cargo.toml | Select-String '^version = "([^"]+)"').Matches.Groups[1].Value)
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $packageName = "OpenCowork-$version-windows-x64-$stamp"
    $outputRoot = Join-Path $projectRoot 'dist\packages'
    $package = Join-Path $outputRoot $packageName
    New-Item -ItemType Directory -Path $package -Force | Out-Null
    foreach ($binary in @('opencowork-shell.exe','opencowork-desktop.exe')) {
        Copy-Item -LiteralPath (Join-Path $buildCache "$Profile\$binary") -Destination $package
    }
    @'
@echo off
setlocal
set "WORKSPACE=%~1"
if not defined WORKSPACE set "WORKSPACE=%~dp0workspace"
if not exist "%WORKSPACE%" mkdir "%WORKSPACE%"
pushd "%WORKSPACE%" || exit /b 1
echo Starting OpenCowork desktop...
start "" "%~dp0opencowork-desktop.exe"
popd
'@ | Set-Content -LiteralPath (Join-Path $package 'Start-OpenCowork.bat') -Encoding Ascii
    @'
# OpenCowork portable desktop

1. Extract the whole ZIP to a writable folder.
2. Double-click Start-OpenCowork.bat. No Rust, Cargo, Python or Node installation is needed.
3. To use an existing project, drag its folder onto Start-OpenCowork.bat, or pass its path as the first argument.
4. Configure a model in Settings > Provider. For screenshot-based control choose a model that accepts image inputs.

Requires Windows x64 and Microsoft Edge WebView2 Evergreen Runtime:
https://developer.microsoft.com/microsoft-edge/webview2/

Computer control is enabled by default. Settings > Permissions includes its switch and optional background memory extraction.
Stopping a chat stops its owned processes. Disabling computer control stops active chats.
Startup errors appear in a dialog. Logs: %LOCALAPPDATA%\OpenCowork\logs.
The workspace holds project settings; session history and extracted memories use the normal OpenCowork config home.
'@ | Set-Content -LiteralPath (Join-Path $package 'README.md') -Encoding UTF8
    Copy-Item -LiteralPath (Join-Path $projectRoot 'third-party\windows-capture-LICENSE.txt') -Destination $package
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs\computer-control.md') -Destination $package
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/autonomous-workflows.md') -Destination $package
    $manifest = Get-ChildItem -LiteralPath $package -File | ForEach-Object { [ordered]@{name=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash} }
    $manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $package 'manifest.json') -Encoding UTF8
    $archive = "$package.zip"
    Compress-Archive -LiteralPath $package -DestinationPath $archive
    Write-Output "Portable package: $archive"
} finally { Pop-Location }
