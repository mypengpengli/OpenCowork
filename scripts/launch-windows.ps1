param(
    [switch]$Rebuild,
    [switch]$BuildOnly
)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\', '/')
function Get-PathId([string]$Path) {
    $digest = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($digest.ComputeHash([Text.Encoding]::UTF8.GetBytes($Path.ToLowerInvariant())))).Replace('-', '').Substring(0, 20) }
    finally { $digest.Dispose() }
}
$checkoutId = Get-PathId $projectRoot
$checkoutRoot = Join-Path $env:LOCALAPPDATA "OpenCowork\checkouts\$checkoutId"
$currentFile = Join-Path $checkoutRoot 'current.txt'
$targetDir = Join-Path $env:LOCALAPPDATA 'OpenClaw\target'
$runtime = $null
if (Test-Path -LiteralPath $currentFile) {
    $runId = [IO.File]::ReadAllText($currentFile).Trim()
    if ($runId -match '^[a-f0-9]{32}$') { $runtime = Join-Path $checkoutRoot "runs\$runId" }
}

try {
    if ($Rebuild -or $BuildOnly -or -not $runtime -or -not (Test-Path -LiteralPath (Join-Path $runtime 'opencowork-shell.exe'))) {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Install Rust and MSVC Build Tools before building. See README.' }
        # Serialize build AND copying: another checkout must not change either binary
        # between Cargo completing and both files being copied into a runtime.
        $mutex = New-Object Threading.Mutex($false, ('Local\OpenCoworkLauncherBuild-' + (Get-PathId $targetDir)))
        $locked = $false
        try {
            Write-Host '[OpenCowork] Building shell and desktop host...'
            try { $locked = $mutex.WaitOne([TimeSpan]::FromMinutes(20)) }
            catch [Threading.AbandonedMutexException] { $locked = $true }
            if (-not $locked) { throw 'Another launcher build is still running. Try again after it finishes.' }
            Push-Location -LiteralPath $projectRoot
            try {
                # A separate profile also avoids EXEs held by older debug launchers.
                & cargo build --locked --profile launcher --target-dir $targetDir -p opencowork-shell -p opencowork-desktop
                if ($LASTEXITCODE -ne 0) { throw 'Build failed. The previous runtime was preserved; no old version was launched.' }
            } finally { Pop-Location }
            $runtime = Join-Path $checkoutRoot ('runs\' + [Guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Path $runtime -Force | Out-Null
            foreach ($name in @('opencowork-shell.exe', 'opencowork-desktop.exe')) {
                Copy-Item -LiteralPath (Join-Path $targetDir "launcher\$name") -Destination (Join-Path $runtime $name)
            }
            # Commit the new runtime only after both files are copied successfully.
            $pending = Join-Path $checkoutRoot ([Guid]::NewGuid().ToString('N') + '.tmp')
            [IO.File]::WriteAllText($pending, (Split-Path -Leaf $runtime))
            if (Test-Path -LiteralPath $currentFile) { [IO.File]::Replace($pending, $currentFile, [NullString]::Value) }
            else { [IO.File]::Move($pending, $currentFile) }
            Write-Host "[OpenCowork] Ready: $runtime"
        } finally {
            if ($locked) { $mutex.ReleaseMutex() }
            $mutex.Dispose()
        }
    }
    if ($BuildOnly) { exit 0 }
    $desktop = Join-Path $runtime 'opencowork-desktop.exe'
    if (Test-Path -LiteralPath $desktop) {
        Write-Host '[OpenCowork] Launching desktop app...'
        Start-Process -FilePath $desktop -WorkingDirectory $projectRoot
    } else {
        Write-Host '[OpenCowork] Desktop host unavailable. Starting web shell...'
        $listener = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback, 0)
        $listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop()
        $start = New-Object Diagnostics.ProcessStartInfo
        $start.FileName = Join-Path $runtime 'opencowork-shell.exe'
        $start.WorkingDirectory = $projectRoot
        $start.UseShellExecute = $false; $start.CreateNoWindow = $true
        $start.EnvironmentVariables['OPENCOWORK_SHELL_PORT'] = [string]$port
        $child = [Diagnostics.Process]::Start($start)
        $url = "http://127.0.0.1:$port"
        $ready = $false
        for ($attempt = 0; $attempt -lt 120; $attempt++) {
            if ($child.HasExited) { throw 'Web shell stopped before becoming ready.' }
            try { $response = Invoke-WebRequest -UseBasicParsing -Uri "$url/api/health" -TimeoutSec 1; if ($response.StatusCode -eq 204) { $ready = $true; break } } catch {}
            Start-Sleep -Milliseconds 250
        }
        if (-not $ready) { throw 'Web shell did not become ready in time.' }
        Start-Process $url
    }
} catch {
    Write-Host "[OpenCowork] $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
