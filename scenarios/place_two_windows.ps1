# T5a two-window proof driver (PowerShell 7).
#
# Launches a founder and a joiner turnstone.exe, each against its own scratch
# profile (LOCALAPPDATA / TURNSTONE_ROOT / PERSONAE_PROFILE isolated per role,
# so the shared personae vault under the real profile is never touched), each
# running its own scenario (scenarios/place_founder.scn,
# scenarios/place_joiner.scn), coordinating through one shared ${EXCHANGE}
# directory for the place card / pre-key / invite files. Waits for both
# scenario.done sentinels, prints their outcome, and exits nonzero unless both
# read RESULT ok.
#
# See design_docs/2026-07-28_turnstone_place_port_plan.md, "T5a. Founder path
# and two-window proof", for the scenario this proves.

param(
    [string]$Exe = "C:/t/turnstone-leave-target/debug/turnstone.exe",
    [string]$Out = "C:/t/turnstone-place-two-windows-$(Get-Date -Format 'yyyyMMdd-HHmmss')",
    [int]$TimeoutSeconds = 600
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not (Test-Path $Exe)) {
    Write-Error "turnstone.exe not found at '$Exe' (build it first, or pass -Exe)"
    exit 1
}

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$exchange = Join-Path $Out "exchange"
New-Item -ItemType Directory -Force -Path $exchange | Out-Null

$roles = "founder", "joiner"
foreach ($role in $roles) {
    $roleDir = Join-Path $Out $role
    New-Item -ItemType Directory -Force -Path (Join-Path $roleDir "appdata") | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $roleDir "root") | Out-Null
}

# Never the user's own profile: every path below is under -Out. Each process
# inherits only this role's environment and logs to <role>/turnstone.log.
function Start-TurnstoneRole {
    param([string]$Role)

    $roleDir = Join-Path $Out $Role
    $roleEnv = @{
        LOCALAPPDATA          = (Join-Path $roleDir "appdata")
        TURNSTONE_ROOT        = (Join-Path $roleDir "root")
        PERSONAE_PROFILE      = $Role
        TURNSTONE_SCENARIO    = (Join-Path $repoRoot "scenarios/place_$Role.scn")
        TURNSTONE_CAPTURE_DIR = $roleDir
        EXCHANGE              = $exchange
    }
    $saved = @{}
    foreach ($key in $roleEnv.Keys) {
        $saved[$key] = [Environment]::GetEnvironmentVariable($key, "Process")
        [Environment]::SetEnvironmentVariable($key, $roleEnv[$key], "Process")
    }
    try {
        Start-Process -FilePath $Exe -PassThru `
            -RedirectStandardOutput (Join-Path $roleDir "turnstone.log") `
            -RedirectStandardError (Join-Path $roleDir "turnstone.err")
    } finally {
        foreach ($key in $roleEnv.Keys) {
            [Environment]::SetEnvironmentVariable($key, $saved[$key], "Process")
        }
    }
}

Write-Host "Launching founder (out: $Out\founder)"
$founderProc = Start-TurnstoneRole -Role "founder"
Write-Host "Launching joiner (out: $Out\joiner)"
$joinerProc = Start-TurnstoneRole -Role "joiner"

$founderDone = Join-Path $Out "founder/scenario.done"
$joinerDone = Join-Path $Out "joiner/scenario.done"

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
$timedOut = $false
while (-not ((Test-Path $founderDone) -and (Test-Path $joinerDone))) {
    if ((Get-Date) -gt $deadline) {
        $timedOut = $true
        break
    }
    if ($founderProc.HasExited -and -not (Test-Path $founderDone)) {
        Write-Warning "founder process exited without writing scenario.done (code $($founderProc.ExitCode))"
        break
    }
    if ($joinerProc.HasExited -and -not (Test-Path $joinerDone)) {
        Write-Warning "joiner process exited without writing scenario.done (code $($joinerProc.ExitCode))"
        break
    }
    Start-Sleep -Milliseconds 500
}

if ($timedOut) {
    Write-Warning "timed out after $TimeoutSeconds s waiting for both scenario.done files"
}

foreach ($proc in @($founderProc, $joinerProc)) {
    if (-not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
}

$ok = $true
foreach ($pair in @(@{Role = "founder"; Path = $founderDone}, @{Role = "joiner"; Path = $joinerDone})) {
    $role = $pair.Role
    $path = $pair.Path
    Write-Host ""
    Write-Host "=== $role scenario.done ($path) ==="
    if (-not (Test-Path $path)) {
        Write-Host "(missing)"
        $ok = $false
        continue
    }
    $lines = Get-Content $path
    $first = $lines | Select-Object -First 1
    Write-Host "first line: $first"
    if ($first -ne "RESULT ok") {
        $ok = $false
    }
    Write-Host "tail:"
    $lines | Select-Object -Last 15 | ForEach-Object { Write-Host "  $_" }
}

foreach ($role in $roles) {
    $roleDir = Join-Path $Out $role
    Write-Host ""
    Write-Host "=== $role captures ==="
    Get-ChildItem -Path $roleDir -Filter "*.png" -ErrorAction SilentlyContinue |
        ForEach-Object { Write-Host "  $($_.Name)" }
    Write-Host "=== $role record-place files ==="
    Get-ChildItem -Path $roleDir -Filter "*.json" -ErrorAction SilentlyContinue |
        ForEach-Object { Write-Host "  $($_.Name)" }
}

if (-not $ok) {
    Write-Host ""
    Write-Error "two-window proof failed: not both scenario.done read RESULT ok"
    exit 1
}

Write-Host ""
Write-Host "two-window proof: both founder and joiner reported RESULT ok"
exit 0
