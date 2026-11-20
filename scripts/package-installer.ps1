param(
    [switch]$SkipBuild,
    [string]$NsisCompiler,
    [string]$WebViewBootstrapper
)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$binaryDir = Join-Path $env:LOCALAPPDATA 'OpenClaw\target\release'
if (-not $NsisCompiler) {
    $candidate = Get-Command makensis.exe -ErrorAction SilentlyContinue
    if ($candidate) { $NsisCompiler = $candidate.Source }
    else { $NsisCompiler = Join-Path $projectRoot '.tmp\installer-tools\nsis-3.13\makensis.exe' }
}
if (-not (Test-Path -LiteralPath $NsisCompiler)) { throw 'Install NSIS 3.13 or pass -NsisCompiler with the path to makensis.exe.' }
if (-not $WebViewBootstrapper) { $WebViewBootstrapper = Join-Path $projectRoot '.tmp\installer-tools\MicrosoftEdgeWebview2Setup.exe' }
if (-not (Test-Path -LiteralPath $WebViewBootstrapper)) { throw 'Pass -WebViewBootstrapper with the Microsoft Evergreen WebView2 bootstrapper.' }
$signature = Get-AuthenticodeSignature -LiteralPath $WebViewBootstrapper
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw 'The WebView2 bootstrapper must have a valid Microsoft signature.' }
Push-Location $projectRoot
try {
    if (-not $SkipBuild) {
        & cargo build --locked --release --target-dir (Join-Path $env:LOCALAPPDATA 'OpenClaw\target') -p opencowork-shell -p opencowork-desktop
        if ($LASTEXITCODE -ne 0) { throw 'Windows application build failed' }
    }
    $version = ((Get-Content Cargo.toml | Select-String '^version = "([^"]+)"').Matches.Groups[1].Value)
    $buildCommit = (& git rev-parse HEAD).Trim()
    $outputDir = Join-Path $projectRoot 'dist\installers'
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
    $output = Join-Path $outputDir "OpenCowork-$version-Setup-x64.exe"
    & $NsisCompiler /V2 /INPUTCHARSET UTF8 "/DAPP_VERSION=$version" "/DBUILD_COMMIT=$buildCommit" "/DPROJECT_ROOT=$projectRoot" "/DBINARY_DIR=$binaryDir" "/DWEBVIEW_BOOTSTRAPPER=$WebViewBootstrapper" "/DOUTPUT_FILE=$output" (Join-Path $PSScriptRoot 'windows-installer.nsi')
    if ($LASTEXITCODE -ne 0) { throw 'Windows installer compilation failed' }
    Write-Output "Installer: $output"
    Write-Output "SHA256: $((Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash)"
} finally { Pop-Location }
