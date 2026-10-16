# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
param(
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9_-]+$')][string]$Scenario,
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]+$')][string]$Case,
    [ValidatePattern('^[a-z0-9_-]*$')][string]$Workshop = '',
    [switch]$Diagnostics
)
$ErrorActionPreference = 'Stop'
$taskRepo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$taskCase = Join-Path $PSScriptRoot "native/$Case"
$taskCapture = Join-Path $taskCase "captures/$Scenario"
$taskFixture = Join-Path $taskRepo "scenarios/$Scenario.scn"
$taskExe = 'C:/t/cargo-targets/turnstone/debug/turnstone.exe'
if (!(Test-Path -LiteralPath $taskFixture)) { throw "Missing scenario $taskFixture" }
if (Test-Path -LiteralPath $taskCapture) { throw "Preserving existing capture owner: $taskCapture" }
New-Item -ItemType Directory -Path $taskCapture -Force | Out-Null
$env:TURNSTONE_ROOT = Join-Path $taskCase 'profile'
$env:MERE_ROOT = Join-Path $taskCase 'mere'
$env:TURNSTONE_CAPTURE_DIR = $taskCapture
$env:TURNSTONE_SCENARIO = $taskFixture
$env:TURNSTONE_THEME_LIBRARY = Join-Path $taskCase 'themes.json'
$env:RUST_LOG = 'warn,turnstone=debug'
$env:RUST_BACKTRACE = '1'
Remove-Item Env:TURNSTONE_SURFACE_POLL_MS -ErrorAction SilentlyContinue
Remove-Item Env:TURNSTONE_A4_DIAGNOSTICS -ErrorAction SilentlyContinue
Remove-Item Env:TURNSTONE_THEME_SCENARIO -ErrorAction SilentlyContinue
Remove-Item Env:TURNSTONE_THEME_CAPTURE_DIR -ErrorAction SilentlyContinue
Remove-Item Env:TURNSTONE_THEME_RECEIPT -ErrorAction SilentlyContinue
if ($Diagnostics) { $env:TURNSTONE_A4_DIAGNOSTICS = '1' }
if ($Workshop) {
    $env:TURNSTONE_THEME_SCENARIO = Join-Path $taskRepo "scenarios/$Workshop.scn"
    $env:TURNSTONE_THEME_CAPTURE_DIR = Join-Path $taskCase "workshop/$Scenario"
    $env:TURNSTONE_THEME_RECEIPT = Join-Path $env:TURNSTONE_THEME_CAPTURE_DIR 'scenario.done'
    New-Item -ItemType Directory -Path $env:TURNSTONE_THEME_CAPTURE_DIR -Force | Out-Null
}
$taskStarted = [DateTime]::UtcNow
& $taskExe *> (Join-Path $taskCase "$Scenario.runtime.log")
$taskExit = $LASTEXITCODE
$taskDonePath = Join-Path $taskCapture 'scenario.done'
$taskDone = if (Test-Path -LiteralPath $taskDonePath) { [IO.File]::ReadAllText($taskDonePath) } else { '' }
$taskOk = $taskExit -eq 0 -and $taskDone.StartsWith('RESULT ok')
[ordered]@{
    scenario = $Scenario; case = $Case; workshop = $Workshop
    started_utc = $taskStarted.ToString('o'); finished_utc = [DateTime]::UtcNow.ToString('o')
    exit = $taskExit; result_ok = $taskOk; default_surface_poll = $true
    executable_sha256 = (Get-FileHash -LiteralPath $taskExe).Hash.ToLowerInvariant()
    fixture_sha256 = (Get-FileHash -LiteralPath $taskFixture).Hash.ToLowerInvariant()
    primary_lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $taskRepo 'Cargo.lock')).Hash.ToLowerInvariant()
} | ConvertTo-Json | Set-Content (Join-Path $taskCase "$Scenario.execution.json")
$taskDone
if (!$taskOk) { throw "Scenario $Scenario failed; preserved $taskDonePath" }
