# T5a/T5b two-window (three-role) proof driver (PowerShell 7).
#
# Launches a founder and a joiner turnstone.exe, each against its own scratch
# profile (LOCALAPPDATA / TURNSTONE_ROOT / PERSONAE_PROFILE isolated per role,
# so the shared personae vault under the real profile is never touched), each
# running its own scenario (scenarios/place_founder.scn,
# scenarios/place_joiner.scn), coordinating through one shared ${EXCHANGE}
# directory for the place card / pre-key / invite files.
#
# Phase 1: founder and joiner exchange one message each way. Phase 2 (step 6
# of the reframe: stop, author while absent, restart, converge): the joiner
# process is killed, the driver signals its absence via
# ${EXCHANGE}\joiner.stopped, the founder authors one more message while the
# joiner is down, then the joiner is relaunched against the SAME
# appdata/root/profile (so the retained place binding reopens offline) running
# scenarios/place_joiner_return.scn, which reconnects, converges the
# absent-authored message, and sends one more. The founder's scenario is one
# long script spanning both phases and only writes its own scenario.done at
# the very end. A third role, reader, launches at the start of phase 2
# alongside the relaunched joiner, running scenarios/place_reader.scn against
# its own scratch profile: it joins the place read-only (T5b, reframe step 7)
# and proves its own write is refused.
#
# T5b (reframe steps 4 and 7) also needs two small static pages to open as
# shared/private addresses; the driver writes shared.html and reader.html into
# ${EXCHANGE} before launching anything, serves that directory over loopback
# HTTP on -PagePort, and exports ${EXCHANGE_URL} (that origin) to every process
# alongside ${EXCHANGE} and ${FOUNDER_DIR}.
#
# Waits for the phase-1 sentinels, then the phase-2 sentinels (founder,
# joiner-return AND reader), prints every outcome, and exits nonzero unless
# founder, joiner (phase 1), joiner-return and reader all read RESULT ok.
#
# T5c (reframe step 5, shared Knot document by projection): the founder is
# given a one-document Knot vault (${Out}\founder\vault\field.knot) and
# TURNSTONE_KNOT_ROOT pointed at it FOR THE FOUNDER PROCESS ONLY, so only the
# founder serves it on the place transport. ${FOUNDER_VAULT_URL} (a
# file:///C:/... address with forward slashes, naming the vault DIRECTORY,
# not the file) is exported to every process so a scenario can
# `open ${FOUNDER_VAULT_URL}/field.knot`. This leg runs after the T5b
# refused-write leg in scenarios/place_founder.scn (see the comment there for
# why), so the founder's scenario.done now depends on both the returning
# joiner's and the reader's Knot-leg markers, not just phase-1 completion:
# the wait below is staged in two steps, first the founder alone (its own
# process exiting is what makes the document holder go away), then the
# returning joiner and the reader together.
#
# See design_docs/2026-07-28_turnstone_place_port_plan.md, "T5a. Founder path
# and two-window proof", "T5b. Shared address and refused write", and "T5c.
# Shared Knot document by projection", for the scenario this proves.

param(
    [string]$Exe = "C:/t/turnstone-leave-target/debug/turnstone.exe",
    [string]$Out = "C:/t/turnstone-place-two-windows-$(Get-Date -Format 'yyyyMMdd-HHmmss')",
    [int]$TimeoutSeconds = 600,
    [int]$PagePort = 43121
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

$roles = "founder", "joiner", "reader"
foreach ($role in $roles) {
    $roleDir = Join-Path $Out $role
    New-Item -ItemType Directory -Force -Path (Join-Path $roleDir "appdata") | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $roleDir "root") | Out-Null
}
New-Item -ItemType Directory -Force -Path (Join-Path $Out "joiner-return") | Out-Null

$founderDir = Join-Path $Out "founder"

# T5c: the founder's own Knot vault, a single small Djot document. Created
# before any process launches, same as the exchange pages below.
$founderVaultDir = Join-Path $founderDir "vault"
New-Item -ItemType Directory -Force -Path $founderVaultDir | Out-Null
$founderVaultDocPath = Join-Path $founderVaultDir "field.knot"
Set-Content -Path $founderVaultDocPath -NoNewline -Value @"
# Shared field notes

Nothing written yet.
"@
$founderVaultUrl = "file:///" + ($founderVaultDir -replace '\\', '/')

# T5b (reframe steps 4 and 7): a shared address the founder opens and shares,
# and a private one the reader opens before its refused write. Written before
# any process launches so every role's wait-file finds them already in place.
$sharedHtmlPath = Join-Path $exchange "shared.html"
$readerHtmlPath = Join-Path $exchange "reader.html"
Set-Content -Path $sharedHtmlPath -NoNewline -Value @"
<!doctype html>
<html>
<head><title>Shared page</title></head>
<body>
<h1>Shared page</h1>
<p>This node is shared by the founder into the place's shared graph.</p>
</body>
</html>
"@
Set-Content -Path $readerHtmlPath -NoNewline -Value @"
<!doctype html>
<html>
<head><title>Reader page</title></head>
<body>
<h1>Reader page</h1>
<p>The reader opens this node and attempts to share it; the write is refused.</p>
</body>
</html>
"@

# The pages are served over loopback HTTP rather than file://: this build
# registers no live-content engine for local files, and the proof needs a
# presented web surface. `${EXCHANGE_URL}/shared.html` is the same address in
# every window, so the shared node reconciles by URL on each side.
$exchangeUrl = "http://127.0.0.1:$PagePort"
$exchangeServerLog = Join-Path $Out "exchange-server.log"
$exchangeServer = Start-Process -FilePath "python" -ArgumentList @(
    "-m", "http.server", "$PagePort", "--bind", "127.0.0.1", "--directory", $exchange
) -PassThru -WindowStyle Hidden -RedirectStandardError $exchangeServerLog
$exchangeReady = $false
for ($attempt = 0; $attempt -lt 100 -and -not $exchangeReady; $attempt++) {
    try {
        Invoke-WebRequest -UseBasicParsing "$exchangeUrl/shared.html" -TimeoutSec 2 | Out-Null
        $exchangeReady = $true
    } catch {
        if ($exchangeServer.HasExited) { break }
        Start-Sleep -Milliseconds 100
    }
}
function Stop-ExchangeServer {
    if ($exchangeServer -and -not $exchangeServer.HasExited) {
        Stop-Process -Id $exchangeServer.Id -Force -ErrorAction SilentlyContinue
    }
}
if (-not $exchangeReady) {
    Stop-ExchangeServer
    Write-Error "exchange page server did not become ready on $exchangeUrl (see $exchangeServerLog)"
    exit 1
}

# Never the user's own profile: every path below is under -Out. `$Role` names
# the appdata/root/profile identity (which persists across a relaunch);
# `$Scenario` and `$CaptureDir` can differ from the role on a later launch of
# the same identity (the joiner-return leg reuses the joiner's appdata/root
# but runs a different scenario into a different capture dir). Each process
# inherits only this launch's environment and logs to <captureDir>/turnstone.log.
function Start-TurnstoneRole {
    param(
        [string]$Role,
        [string]$Scenario = $null,
        [string]$CaptureDir = $null
    )

    $roleDir = Join-Path $Out $Role
    if (-not $Scenario) { $Scenario = Join-Path $repoRoot "scenarios/place_$Role.scn" }
    if (-not $CaptureDir) { $CaptureDir = $roleDir }
    New-Item -ItemType Directory -Force -Path $CaptureDir | Out-Null

    $roleEnv = @{
        LOCALAPPDATA          = (Join-Path $roleDir "appdata")
        TURNSTONE_ROOT        = (Join-Path $roleDir "root")
        PERSONAE_PROFILE      = $Role
        TURNSTONE_SCENARIO    = $Scenario
        TURNSTONE_CAPTURE_DIR = $CaptureDir
        EXCHANGE              = $exchange
        EXCHANGE_URL          = $exchangeUrl
        FOUNDER_DIR           = $founderDir
        # T5c: every process can name the founder's vault by address, but
        # only the founder itself is given TURNSTONE_KNOT_ROOT, so only the
        # founder's worker serves it on the place transport.
        FOUNDER_VAULT_URL     = $founderVaultUrl
    }
    if ($Role -eq "founder") {
        $roleEnv["TURNSTONE_KNOT_ROOT"] = $founderVaultDir
    } else {
        # Run 24 showed non-founder children inheriting the variable as an
        # empty string; remove it outright so no vault-less role reads a path.
        Remove-Item Env:TURNSTONE_KNOT_ROOT -ErrorAction SilentlyContinue
    }
    $saved = @{}
    foreach ($key in $roleEnv.Keys) {
        $saved[$key] = [Environment]::GetEnvironmentVariable($key, "Process")
        [Environment]::SetEnvironmentVariable($key, $roleEnv[$key], "Process")
    }
    # Record what this child actually inherits, so a stray TURNSTONE_* value
    # in a run is attributable from the run directory alone.
    Get-ChildItem Env: | Where-Object Name -like "TURNSTONE_*" |
        ForEach-Object { "$($_.Name)=$($_.Value)" } |
        Set-Content -Path (Join-Path $CaptureDir "launch-env.txt")
    try {
        Start-Process -FilePath $Exe -PassThru `
            -RedirectStandardOutput (Join-Path $CaptureDir "turnstone.log") `
            -RedirectStandardError (Join-Path $CaptureDir "turnstone.err")
    } finally {
        foreach ($key in $roleEnv.Keys) {
            [Environment]::SetEnvironmentVariable($key, $saved[$key], "Process")
        }
    }
}

function Wait-ForFiles {
    param([string[]]$Paths, [int]$TimeoutSeconds, [scriptblock]$OnPoll = $null)

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ($true) {
        $allExist = $true
        foreach ($p in $Paths) {
            if (-not (Test-Path $p)) { $allExist = $false; break }
        }
        if ($allExist) { return $true }
        if ((Get-Date) -gt $deadline) { return $false }
        if ($OnPoll) { & $OnPoll }
        Start-Sleep -Milliseconds 500
    }
}

function Report-Done {
    param([string]$Role, [string]$Path)

    Write-Host ""
    Write-Host "=== $Role scenario.done ($Path) ==="
    if (-not (Test-Path $Path)) {
        Write-Host "(missing)"
        return $false
    }
    $lines = Get-Content $Path
    $first = $lines | Select-Object -First 1
    Write-Host "first line: $first"
    Write-Host "tail:"
    $lines | Select-Object -Last 15 | ForEach-Object { Write-Host "  $_" }
    return $first -eq "RESULT ok"
}

Write-Host "Launching founder (out: $Out\founder)"
$founderProc = Start-TurnstoneRole -Role "founder"
Write-Host "Launching joiner (out: $Out\joiner)"
$joinerProc = Start-TurnstoneRole -Role "joiner"

# Phase-1 gate: the joiner's scenario really finishes here (its own
# scenario.done), but the founder's spans both phases, so its phase-1
# checkpoint is the founder_phase1 record-place file instead.
$joinerDone = Join-Path $Out "joiner/scenario.done"
$founderPhase1 = Join-Path $Out "founder/founder_phase1.json"

$phase1Ok = Wait-ForFiles -Paths @($joinerDone, $founderPhase1) -TimeoutSeconds $TimeoutSeconds -OnPoll {
    if ($founderProc.HasExited -and -not (Test-Path $founderPhase1)) {
        Write-Warning "founder process exited before phase 1 completed (code $($founderProc.ExitCode))"
    }
    if ($joinerProc.HasExited -and -not (Test-Path $joinerDone)) {
        Write-Warning "joiner process exited without writing scenario.done (code $($joinerProc.ExitCode))"
    }
}

if (-not $phase1Ok) {
    Write-Warning "timed out after $TimeoutSeconds s waiting for phase 1 (joiner scenario.done + founder_phase1.json)"
}

$joinerOk = Report-Done -Role "joiner" -Path $joinerDone
if (-not (Test-Path $founderPhase1)) {
    Write-Host ""
    Write-Host "=== founder founder_phase1.json ($founderPhase1) ==="
    Write-Host "(missing)"
}

if (-not $phase1Ok) {
    foreach ($proc in @($founderProc, $joinerProc)) {
        if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    }
    Stop-ExchangeServer
    Write-Error "two-window proof failed: phase 1 did not complete"
    exit 1
}

# Phase 2 (step 6, plus T5b steps 4 and 7): stop the joiner, signal its
# absence, let the founder author while it is gone, then restart the joiner
# against the SAME appdata/root/profile running the return scenario. The
# reader launches here too, on its own scratch profile, running
# scenarios/place_reader.scn.
Write-Host ""
Write-Host "Phase 2: stopping joiner, authoring while absent, restarting, admitting reader..."
if (-not $joinerProc.HasExited) {
    Stop-Process -Id $joinerProc.Id -Force -ErrorAction SilentlyContinue
}
Set-Content -Path (Join-Path $exchange "joiner.stopped") -Value "stopped"

$joinerReturnDir = Join-Path $Out "joiner-return"
Write-Host "Relaunching joiner as joiner-return (out: $joinerReturnDir)"
$joinerReturnProc = Start-TurnstoneRole -Role "joiner" `
    -Scenario (Join-Path $repoRoot "scenarios/place_joiner_return.scn") `
    -CaptureDir $joinerReturnDir

Write-Host "Launching reader (out: $Out\reader)"
$readerProc = Start-TurnstoneRole -Role "reader"

$founderDone = Join-Path $Out "founder/scenario.done"
$joinerReturnDone = Join-Path $joinerReturnDir "scenario.done"
$readerDone = Join-Path $Out "reader/scenario.done"

$remaining = [math]::Max(30, $TimeoutSeconds)

# T5c: the founder's own scenario.done comes FIRST and on its own. The
# founder's process exiting (right after its scenario ends) is what makes it
# stop serving its Knot vault, and the returning joiner's remaining steps
# (observing that unavailability) only make sense once that has happened, so
# staging the wait this way (founder alone, THEN joiner-return and reader
# together) matches the real dependency instead of merely waiting on all
# three files with no ordering meaning between them.
Write-Host "Waiting for the founder to finish (its own exit is the Knot holder going away)..."
$founderStageOk = Wait-ForFiles -Paths @($founderDone) -TimeoutSeconds $remaining -OnPoll {
    if ($founderProc.HasExited -and -not (Test-Path $founderDone)) {
        Write-Warning "founder process exited without writing scenario.done (code $($founderProc.ExitCode))"
    }
}
if (-not $founderStageOk) {
    Write-Warning "timed out after $remaining s waiting for founder scenario.done"
}

Write-Host "Waiting for joiner-return and reader to finish..."
$restStageOk = Wait-ForFiles -Paths @($joinerReturnDone, $readerDone) -TimeoutSeconds $remaining -OnPoll {
    if ($joinerReturnProc.HasExited -and -not (Test-Path $joinerReturnDone)) {
        Write-Warning "joiner-return process exited without writing scenario.done (code $($joinerReturnProc.ExitCode))"
    }
    if ($readerProc.HasExited -and -not (Test-Path $readerDone)) {
        Write-Warning "reader process exited without writing scenario.done (code $($readerProc.ExitCode))"
    }
}
if (-not $restStageOk) {
    Write-Warning "timed out after $remaining s waiting for joiner-return and reader scenario.done"
}

$phase2Ok = $founderStageOk -and $restStageOk

foreach ($proc in @($founderProc, $joinerReturnProc, $readerProc)) {
    if (-not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
}

$founderOk = Report-Done -Role "founder" -Path $founderDone
$joinerReturnOk = Report-Done -Role "joiner-return" -Path $joinerReturnDone
$readerOk = Report-Done -Role "reader" -Path $readerDone

foreach ($pair in @(@{Role = "founder"; Dir = $founderDir}, @{Role = "joiner"; Dir = (Join-Path $Out "joiner")}, @{Role = "joiner-return"; Dir = $joinerReturnDir}, @{Role = "reader"; Dir = (Join-Path $Out "reader")})) {
    $roleDir = $pair.Dir
    Write-Host ""
    Write-Host "=== $($pair.Role) captures ==="
    Get-ChildItem -Path $roleDir -Filter "*.png" -ErrorAction SilentlyContinue |
        ForEach-Object { Write-Host "  $($_.Name)" }
    Write-Host "=== $($pair.Role) record-place files ==="
    Get-ChildItem -Path $roleDir -Filter "*.json" -ErrorAction SilentlyContinue |
        ForEach-Object { Write-Host "  $($_.Name)" }
}

$ok = $phase1Ok -and $phase2Ok -and $founderOk -and $joinerOk -and $joinerReturnOk -and $readerOk
Stop-ExchangeServer

if (-not $ok) {
    Write-Host ""
    Write-Error "two-window proof failed: not all of founder, joiner, joiner-return and reader read RESULT ok"
    exit 1
}

Write-Host ""
Write-Host "two-window proof: founder, joiner, joiner-return and reader all reported RESULT ok"
exit 0
