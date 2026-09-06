# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

param(
    [Parameter(Mandatory = $true)]
    [string] $TurnstoneBin,

    [string] $OutputRoot = (Join-Path ([System.IO.Path]::GetTempPath()) (
        "turnstone-reader-appearance-" + (Get-Date -Format "yyyyMMdd-HHmmss")
    ))
)

$ErrorActionPreference = "Stop"
$scenarioRoot = $PSScriptRoot
$repositoryRoot = Split-Path -Parent $scenarioRoot
$scenario = Join-Path $scenarioRoot "reader_appearance.scn"
$fixture = Join-Path $scenarioRoot "fixtures\browser_zoom_server.ps1"
$binaryInput = if ([System.IO.Path]::IsPathRooted($TurnstoneBin)) {
    $TurnstoneBin
} else {
    Join-Path $repositoryRoot $TurnstoneBin
}
$binary = [System.IO.Path]::GetFullPath($binaryInput)
foreach ($path in @($binary, $scenario, $fixture)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required Reader appearance path does not exist: $path"
    }
}

[System.IO.Directory]::CreateDirectory($OutputRoot) | Out-Null
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
$profileRoot = Join-Path $OutputRoot "profile"
$captureRoot = Join-Path $OutputRoot "capture"
foreach ($freshRoot in @($profileRoot, $captureRoot)) {
    if (Test-Path -LiteralPath $freshRoot) {
        throw "Reader appearance receipt requires a fresh path: $freshRoot"
    }
    [System.IO.Directory]::CreateDirectory($freshRoot) | Out-Null
}

$ready = Join-Path $OutputRoot "fixture.ready"
$serverDone = Join-Path $OutputRoot "fixture.done"
$scenarioDone = Join-Path $captureRoot "scenario.done"
$expectedCaptures = @(
    "01_two_reader_appearances.png",
    "02_inset_scrolled.png",
    "03_independent_scrolls.png",
    "04_inset_survives.png"
)
$expectedObservations = @(
    "01_before_scroll.txt",
    "02_inset_only.txt",
    "03_both_scrolled.txt",
    "04_inset_survives.txt"
)

function Read-ReaderAppearances {
    param([string] $Path)

    $lines = Get-Content -LiteralPath $Path
    if ($lines.Count -lt 2 -or $lines[0] -ne "RESULT ok") {
        throw "Reader observation is malformed: $Path"
    }
    $pattern = 'id=(?<id>0x[0-9a-f]+) role=(?<role>\w+) source=(?<source>\d+) rect=(?<x>-?[\d.]+),(?<y>-?[\d.]+) (?<rectWidth>[\d.]+)x(?<rectHeight>[\d.]+) viewport=(?<width>\d+)x(?<height>\d+) scroll=(?<scroll>-?[\d.]+)'
    $invariant = [System.Globalization.CultureInfo]::InvariantCulture
    $records = @([regex]::Matches(($lines[1..($lines.Count - 1)] -join " "), $pattern) | ForEach-Object {
        [pscustomobject]@{
            Id = $_.Groups['id'].Value
            Role = $_.Groups['role'].Value
            Source = [int] $_.Groups['source'].Value
            Width = [int] $_.Groups['width'].Value
            Height = [int] $_.Groups['height'].Value
            Scroll = [double]::Parse($_.Groups['scroll'].Value, $invariant)
        }
    })
    if ($records.Count -eq 0) { throw "Reader observation has no records: $Path" }
    return $records
}

function Reader-Role {
    param([object[]] $Records, [string] $Role, [string] $Label)

    $matches = @($Records | Where-Object { $_.Role -eq $Role })
    if ($matches.Count -ne 1) { throw "$Label expected one $Role record, got $($matches.Count)" }
    return $matches[0]
}

function Assert-ExactNumber {
    param([double] $Actual, [double] $Expected, [string] $Label)

    if ($Actual -ne $Expected) {
        throw "$Label changed from $Expected to $Actual"
    }
}

$previousRoot = [Environment]::GetEnvironmentVariable("TURNSTONE_ROOT", "Process")
$previousScenario = [Environment]::GetEnvironmentVariable("TURNSTONE_SCENARIO", "Process")
$previousCapture = [Environment]::GetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", "Process")
$server = $null
try {
    $server = Start-Process -FilePath (Join-Path $PSHOME "pwsh.exe") -ArgumentList @(
        "-NoProfile", "-File", $fixture, "-Port", "43118", "-ReadyPath", $ready,
        "-ReceiptPath", $serverDone, "-TimeoutSeconds", "180"
    ) -WindowStyle Hidden -PassThru
    for ($attempt = 0; $attempt -lt 200 -and -not (Test-Path -LiteralPath $ready); $attempt++) {
        if ($server.HasExited) { throw "Reader fixture exited before becoming ready" }
        Start-Sleep -Milliseconds 50
    }
    if (-not (Test-Path -LiteralPath $ready)) { throw "Reader fixture did not become ready" }

    [Environment]::SetEnvironmentVariable("TURNSTONE_ROOT", $profileRoot, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_SCENARIO", $scenario, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", $captureRoot, "Process")
    Push-Location $repositoryRoot
    try {
        & $binary
        $appExit = $LASTEXITCODE
    }
    finally { Pop-Location }

    Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:43118/stop" | Out-Null
    if (-not $server.WaitForExit(10000)) { throw "Reader fixture did not stop" }
    if ($server.ExitCode -ne 0) { throw "Reader fixture exited $($server.ExitCode)" }

    if (-not (Test-Path -LiteralPath $scenarioDone)) { throw "Reader scenario produced no scenario.done" }
    $scenarioResult = Get-Content -LiteralPath $scenarioDone
    if ($scenarioResult.Count -eq 0 -or $scenarioResult[0] -ne "RESULT ok") {
        throw "Reader scenario failed:`n$($scenarioResult -join "`n")"
    }
    if ($appExit -ne 0) { throw "Reader scenario returned process exit $appExit after RESULT ok" }
    if (-not (Test-Path -LiteralPath $serverDone)) { throw "Reader fixture produced no receipt" }
    if ((Get-Content -LiteralPath $serverDone)[0] -ne "RESULT ok") {
        throw "Reader fixture failed:`n$((Get-Content -LiteralPath $serverDone) -join "`n")"
    }

    foreach ($captureName in $expectedCaptures) {
        $capture = Join-Path $captureRoot $captureName
        if (-not (Test-Path -LiteralPath $capture -PathType Leaf)) { throw "missing capture: $captureName" }
        [byte[]] $png = [System.IO.File]::ReadAllBytes($capture)
        [byte[]] $signature = @(137, 80, 78, 71, 13, 10, 26, 10)
        if ($png.Length -lt 24 -or (($png[0..7] -join ',') -ne ($signature -join ','))) {
            throw "capture is not a PNG: $captureName"
        }
    }
    foreach ($observationName in $expectedObservations) {
        $observation = Join-Path $captureRoot $observationName
        if (-not (Test-Path -LiteralPath $observation -PathType Leaf)) {
            throw "missing Reader observation: $observationName"
        }
        if ((Get-Content -LiteralPath $observation)[0] -ne "RESULT ok") {
            throw "Reader observation failed: $observationName"
        }
    }
    $before = Read-ReaderAppearances (Join-Path $captureRoot "01_before_scroll.txt")
    $afterInset = Read-ReaderAppearances (Join-Path $captureRoot "02_inset_only.txt")
    $afterBoth = Read-ReaderAppearances (Join-Path $captureRoot "03_both_scrolled.txt")
    $survivor = Read-ReaderAppearances (Join-Path $captureRoot "04_inset_survives.txt")
    if ($before.Count -ne 2) { throw "initial observation expected two Reader appearances" }
    $beforeInset = Reader-Role $before "inset" "initial observation"
    $beforeWorkbench = Reader-Role $before "workbench" "initial observation"
    if ($beforeInset.Id -eq $beforeWorkbench.Id) { throw "Reader appearances share a surface id" }
    if ($beforeInset.Source -ne $beforeWorkbench.Source) { throw "Reader appearances do not share one source group" }
    if ($beforeInset.Width -eq $beforeWorkbench.Width) { throw "Reader appearances did not receive different viewport widths" }

    $insetOnlyInset = Reader-Role $afterInset "inset" "inset-only observation"
    $insetOnlyWorkbench = Reader-Role $afterInset "workbench" "inset-only observation"
    if ($insetOnlyInset.Id -ne $beforeInset.Id -or $insetOnlyWorkbench.Id -ne $beforeWorkbench.Id) {
        throw "Reader appearance id changed while both presentations remained live"
    }
    if ($insetOnlyInset.Scroll -le $beforeInset.Scroll) { throw "inset scroll did not advance" }
    Assert-ExactNumber $insetOnlyWorkbench.Scroll $beforeWorkbench.Scroll "workbench scroll after inset input"

    $bothInset = Reader-Role $afterBoth "inset" "both-scrolled observation"
    $bothWorkbench = Reader-Role $afterBoth "workbench" "both-scrolled observation"
    if ($bothInset.Id -ne $beforeInset.Id -or $bothWorkbench.Id -ne $beforeWorkbench.Id) {
        throw "Reader appearance id changed during independent scrolling"
    }
    Assert-ExactNumber $bothInset.Scroll $insetOnlyInset.Scroll "inset scroll after Workbench input"
    if ($bothWorkbench.Scroll -le $insetOnlyWorkbench.Scroll) { throw "Workbench scroll did not advance" }

    if ($survivor.Count -ne 1) { throw "close observation expected one Reader appearance" }
    $survivorInset = Reader-Role $survivor "inset" "close observation"
    if ($survivorInset.Id -ne $beforeInset.Id) { throw "inset surface id changed after Workbench close" }
    Assert-ExactNumber $survivorInset.Scroll $bothInset.Scroll "inset scroll after Workbench close"

    $acceptance = Join-Path $OutputRoot "acceptance.done"
    [System.IO.File]::WriteAllLines($acceptance, @(
        "RESULT ok",
        "scenario=$scenario",
        "scenario-receipt=$scenarioDone",
        "fixture-receipt=$serverDone",
        "captures=$($expectedCaptures -join ',')",
        "observations=$($expectedObservations -join ',')"
    ))
    Get-Content -LiteralPath $acceptance
}
finally {
    if ($null -ne $server -and -not $server.HasExited) { Stop-Process -Id $server.Id -Force }
    [Environment]::SetEnvironmentVariable("TURNSTONE_ROOT", $previousRoot, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_SCENARIO", $previousScenario, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", $previousCapture, "Process")
}
