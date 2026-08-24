<#
.SYNOPSIS
    The command palette open-lag receipt: closed versus open, per stage.

.DESCRIPTION
    Runs the four palette_lag_*.scn conditions N times each against ONE binary,
    with the shell's per-frame stage log enabled, and reports median and p95 for
    every stage rather than for the frame as a whole. This exists because the
    2026-08-22 diagnostic could reproduce roughly a 9x whole-frame delta with
    the palette open and attribute none of it: `frame_ms` alone names a slow
    frame without naming the slow stage.

    Pass a RELEASE binary. The 2026-08-22 numbers came from an unoptimized
    build and are evidence for the symptom, not a baseline; a receipt taken on
    a debug build would restate that mistake. The script warns rather than
    refuses, because an intentional debug comparison is a legitimate thing to
    want.

    Each run gets a fresh TURNSTONE_ROOT profile, so no run inherits another's
    session, layout, or settings. All four scenarios are offline (mere://
    addresses fetch nothing), so the numbers describe the chrome path.

    The counters matter as much as the milliseconds. The note's done condition
    is that a merely-open palette provokes no repeated state, layout, or raster
    work, so suggestion_runs, chrome_syncs, and dirty_tiles summed across an
    idle block are the part of this receipt that can read zero.

.PARAMETER TurnstoneBin
    Path to the turnstone executable under test.

.PARAMETER Runs
    Repetitions per condition. Three is what the note's receipt section asks
    for; more narrows the p95.

.EXAMPLE
    ./run_palette_lag.ps1 -TurnstoneBin ../target/release/turnstone.exe
#>
param(
    [Parameter(Mandatory = $true)]
    [string] $TurnstoneBin,

    [ValidateRange(1, 50)]
    [int] $Runs = 3,

    [ValidateSet("closed", "open", "keys", "edit")]
    [string[]] $Only = @(),

    [string] $OutputRoot = (Join-Path (
        [System.IO.Path]::GetTempPath()
    ) ("turnstone-palette-lag-" + (Get-Date -Format "yyyyMMdd-HHmmss")))
)

$ErrorActionPreference = "Stop"

$scenarioRoot = $PSScriptRoot
$repositoryRoot = Split-Path -Parent $scenarioRoot
# Resolve against the PowerShell location, not the process working directory:
# [System.IO.Path]::GetFullPath uses the latter, which Set-Location does not
# move, so the relative path in the example above would resolve somewhere the
# caller never meant.
$binary = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($TurnstoneBin)
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "Turnstone binary does not exist: $binary"
}
if ($binary -match '[\\/]debug[\\/]') {
    Write-Warning "This looks like a debug build. The note's receipt asks for a release build; a debug number is evidence for the symptom, not a baseline."
}
$OutputRoot = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputRoot)
[System.IO.Directory]::CreateDirectory($OutputRoot) | Out-Null

# TailFrames: how many frame lines are the MEASURED block. Each scenario places
# its block at the end and `settle N` pumps exactly N frames, so the window is
# deterministic; the numbers here must move with the .scn files, which is why
# each file states its own block length in its header comment.
#
# TrailingSkip drops the frames a scenario spends after its block - every file
# ends with `log`, and a step is a frame. Without it, one frame of the wrong
# condition would sit inside every sample.
$cases = @(
    [pscustomobject]@{ Name = "closed"; Scenario = "palette_lag_closed.scn"; TailFrames = 120; TrailingSkip = 1; Label = "palette closed, idle" }
    [pscustomobject]@{ Name = "open";   Scenario = "palette_lag_open.scn";   TailFrames = 120; TrailingSkip = 1; Label = "palette open, idle" }
    [pscustomobject]@{ Name = "keys";   Scenario = "palette_lag_keys.scn";   TailFrames = 50;  TrailingSkip = 1; Label = "palette open, row selection" }
    [pscustomobject]@{ Name = "edit";   Scenario = "palette_lag_edit.scn";   TailFrames = 45;  TrailingSkip = 1; Label = "palette open, text edits" }
)
if ($Only.Count -gt 0) {
    $cases = $cases | Where-Object { $Only -contains $_.Name }
}

# The stage fields the shell emits, in pipeline order. frame_ms leads because
# the parts are only trustworthy if they roughly account for the whole.
$durationFields = @(
    "frame_ms",
    "suggestions_ms",
    "chrome_sync_ms",
    "chrome_scene_ms",
    "pane_scenes_ms",
    "raster_ms",
    "compose_ms"
)
$countFields = @(
    "suggestion_runs",
    "suggestion_refits",
    "chrome_syncs",
    "rasterized",
    "dirty_tiles",
    "surfaces"
)

function Get-Percentile {
    param([double[]] $Values, [double] $Percentile)
    if ($Values.Count -eq 0) { return [double]::NaN }
    $sorted = $Values | Sort-Object
    $index = [Math]::Ceiling($Percentile * $sorted.Count) - 1
    if ($index -lt 0) { $index = 0 }
    if ($index -ge $sorted.Count) { $index = $sorted.Count - 1 }
    return [double] $sorted[$index]
}

# One `frame` log line into a hashtable of its numeric fields. Parsed by field
# name rather than by position, so adding a stage to the log cannot silently
# shift this script onto the wrong column.
function Read-FrameLine {
    param([string] $Line)
    # tracing-subscriber emboldens field names when ANSI is on, which would
    # otherwise glue an escape sequence onto the front of every key and make
    # every field lookup miss. Strip the sequences before parsing rather than
    # depending on the writer having detected a non-terminal.
    $escape = [char] 27
    $Line = [regex]::Replace($Line, ($escape + '\[[0-9;]*[A-Za-z]'), "")
    if ($Line -notmatch 'frame_ms=') { return $null }
    $frame = @{}
    foreach ($match in [regex]::Matches($Line, '(?<key>[a-z_]+)=(?<value>-?\d+(?:\.\d+)?(?:e-?\d+)?)')) {
        $frame[$match.Groups['key'].Value] = [double] $match.Groups['value'].Value
    }
    if ($Line -match 'palette=(?<open>true|false)') {
        $frame['palette_open'] = ($Matches['open'] -eq 'true')
    }
    if (-not $frame.ContainsKey('frame_ms')) { return $null }
    return $frame
}

$previousRoot = [Environment]::GetEnvironmentVariable("TURNSTONE_ROOT", "Process")
$previousScenario = [Environment]::GetEnvironmentVariable("TURNSTONE_SCENARIO", "Process")
$previousCapture = [Environment]::GetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", "Process")
$previousLog = [Environment]::GetEnvironmentVariable("RUST_LOG", "Process")

$samples = @{}
$frameRows = [System.Collections.Generic.List[object]]::new()

try {
    foreach ($case in $cases) {
        $samples[$case.Name] = [System.Collections.Generic.List[hashtable]]::new()
        for ($run = 1; $run -le $Runs; $run++) {
            $runRoot = Join-Path $OutputRoot ("{0}-run{1}" -f $case.Name, $run)
            $profileRoot = Join-Path $runRoot "profile"
            $captureRoot = Join-Path $runRoot "capture"
            [System.IO.Directory]::CreateDirectory($profileRoot) | Out-Null
            [System.IO.Directory]::CreateDirectory($captureRoot) | Out-Null
            $logPath = Join-Path $runRoot "frames.log"
            $scenarioDone = Join-Path $captureRoot "scenario.done"

            [Environment]::SetEnvironmentVariable("TURNSTONE_ROOT", $profileRoot, "Process")
            [Environment]::SetEnvironmentVariable(
                "TURNSTONE_SCENARIO",
                (Join-Path $scenarioRoot $case.Scenario),
                "Process"
            )
            [Environment]::SetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", $captureRoot, "Process")
            # Warnings stay visible; the frame lane goes to debug. Anything
            # noisier buries the frame lines this script parses.
            [Environment]::SetEnvironmentVariable(
                "RUST_LOG",
                "turnstone=warn,turnstone::shell::render=debug",
                "Process"
            )

            Write-Host ("RUN {0} ({1}/{2}) - {3}" -f $case.Name, $run, $Runs, $case.Label)
            Push-Location $repositoryRoot
            try {
                $process = Start-Process -FilePath $binary -NoNewWindow -Wait -PassThru -RedirectStandardOutput $logPath
                $appExit = $process.ExitCode
            }
            finally {
                Pop-Location
            }

            if (-not (Test-Path -LiteralPath $scenarioDone)) {
                throw "$($case.Name) run $run produced no scenario.done"
            }
            $scenarioResult = Get-Content -LiteralPath $scenarioDone
            if ($scenarioResult[0] -ne "RESULT ok") {
                throw ("{0} run {1} failed:{2}{3}" -f $case.Name, $run, [Environment]::NewLine, ($scenarioResult -join [Environment]::NewLine))
            }
            if ($appExit -ne 0) {
                throw "$($case.Name) run $run returned process exit $appExit after RESULT ok"
            }

            $frames = [System.Collections.Generic.List[hashtable]]::new()
            foreach ($line in [System.IO.File]::ReadLines($logPath)) {
                $frame = Read-FrameLine -Line $line
                if ($null -ne $frame) { $frames.Add($frame) | Out-Null }
            }
            if ($frames.Count -eq 0) {
                throw "$($case.Name) run $run logged no frames. Is this binary built from a tree that carries the stage log?"
            }
            $needed = $case.TailFrames + $case.TrailingSkip
            if ($frames.Count -lt $needed) {
                throw ("{0} run {1} logged {2} frames, fewer than the {3} its measured block plus trailing frames need. The scenario and this table have drifted apart." -f $case.Name, $run, $frames.Count, $needed)
            }

            # The measured block sits at the end, minus whatever the scenario
            # spends after it: everything BEFORE it is boot, page-open, and
            # warm-up, whose cost is real but is not what is being compared.
            $last = $frames.Count - 1 - $case.TrailingSkip
            $measured = $frames[($last - $case.TailFrames + 1)..$last]

            # An open condition whose measured frames are not actually open
            # would compare the control against itself.
            $expectOpen = $case.Name -ne "closed"
            $wrongState = @($measured | Where-Object { $_['palette_open'] -ne $expectOpen })
            if ($wrongState.Count -gt 0) {
                throw ("{0} run {1}: {2} of {3} measured frames had the wrong palette state, expected open={4}. The measured block is not the condition it claims." -f $case.Name, $run, $wrongState.Count, $measured.Count, $expectOpen)
            }

            foreach ($frame in $measured) {
                $samples[$case.Name].Add($frame) | Out-Null
                $row = [ordered]@{ case = $case.Name; run = $run }
                foreach ($field in ($durationFields + $countFields)) {
                    $row[$field] = if ($frame.ContainsKey($field)) { $frame[$field] } else { $null }
                }
                $frameRows.Add([pscustomobject] $row) | Out-Null
            }
            Write-Host ("    {0} frames logged, {1} measured" -f $frames.Count, $measured.Count)
        }
    }
}
finally {
    [Environment]::SetEnvironmentVariable("TURNSTONE_ROOT", $previousRoot, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_SCENARIO", $previousScenario, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", $previousCapture, "Process")
    [Environment]::SetEnvironmentVariable("RUST_LOG", $previousLog, "Process")
}

$csvPath = Join-Path $OutputRoot "frames.csv"
$frameRows | Export-Csv -LiteralPath $csvPath -NoTypeInformation

$report = [System.Collections.Generic.List[string]]::new()
$report.Add("Command palette open-lag receipt") | Out-Null
$report.Add("binary: $binary") | Out-Null
$report.Add("runs per condition: $Runs") | Out-Null
$report.Add("") | Out-Null

foreach ($field in $durationFields) {
    $report.Add(("{0,-18} {1,12} {2,12}   (ms)" -f $field, "median", "p95")) | Out-Null
    foreach ($case in $cases) {
        $values = @($samples[$case.Name] | ForEach-Object { if ($_.ContainsKey($field)) { [double] $_[$field] } else { 0.0 } })
        $median = Get-Percentile -Values $values -Percentile 0.5
        $p95 = Get-Percentile -Values $values -Percentile 0.95
        $report.Add(("  {0,-16} {1,12:F3} {2,12:F3}" -f $case.Name, $median, $p95)) | Out-Null
    }
    $report.Add("") | Out-Null
}

$report.Add("Repeated-work counters, summed across every measured frame.") | Out-Null
$report.Add("An idle condition that rebuilds nothing reads zero for the first three.") | Out-Null
$report.Add(("  {0,-16} {1}" -f "condition", (($countFields | ForEach-Object { "{0,18}" -f $_ }) -join ""))) | Out-Null
foreach ($case in $cases) {
    $totals = foreach ($field in $countFields) {
        $sum = ($samples[$case.Name] | ForEach-Object { if ($_.ContainsKey($field)) { [double] $_[$field] } else { 0.0 } } | Measure-Object -Sum).Sum
        "{0,18}" -f [long] $sum
    }
    $report.Add(("  {0,-16} {1}" -f $case.Name, ($totals -join ""))) | Out-Null
}
$report.Add("") | Out-Null

# The headline the note asked for, stated only when both sides were measured.
if ($samples.ContainsKey("closed") -and $samples.ContainsKey("open")) {
    $closedMedian = Get-Percentile -Values @($samples["closed"] | ForEach-Object { [double] $_['frame_ms'] }) -Percentile 0.5
    $openMedian = Get-Percentile -Values @($samples["open"] | ForEach-Object { [double] $_['frame_ms'] }) -Percentile 0.5
    if ($closedMedian -gt 0) {
        $report.Add(("closed-versus-open whole-frame ratio (median): {0:F2}x" -f ($openMedian / $closedMedian))) | Out-Null
    }
    $report.Add("Per-stage deltas, open idle minus closed idle (median ms):") | Out-Null
    foreach ($field in $durationFields) {
        if ($field -eq "frame_ms") { continue }
        $closed = Get-Percentile -Values @($samples["closed"] | ForEach-Object { if ($_.ContainsKey($field)) { [double] $_[$field] } else { 0.0 } }) -Percentile 0.5
        $open = Get-Percentile -Values @($samples["open"] | ForEach-Object { if ($_.ContainsKey($field)) { [double] $_[$field] } else { 0.0 } }) -Percentile 0.5
        $report.Add(("  {0,-18} {1,10:F3}" -f $field, ($open - $closed))) | Out-Null
    }
    $report.Add("") | Out-Null
}

$report.Add("Not covered here: input-to-present latency. The frame log times the") | Out-Null
$report.Add("frame, not the wait from keystroke to photons; that needs its own") | Out-Null
$report.Add("instrument and is still open in the note.") | Out-Null

$reportPath = Join-Path $OutputRoot "receipt.txt"
$report | Set-Content -LiteralPath $reportPath -Encoding utf8
$report | ForEach-Object { Write-Host $_ }

Write-Host ""
Write-Host "per-frame rows: $csvPath"
Write-Host "receipt:        $reportPath"
