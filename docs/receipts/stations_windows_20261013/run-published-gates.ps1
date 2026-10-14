# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
param([switch]$Build)
$ErrorActionPreference = 'Stop'
$taskRepository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
Set-Location $taskRepository
$env:CARGO_TARGET_DIR = 'C:/t/cargo-targets/turnstone'
$env:CARGO_BUILD_BUILD_DIR = 'C:/t/cargo-targets/turnstone/build'
$env:CARGO_NET_OFFLINE = 'true'
$taskManifestHash = (Get-FileHash -LiteralPath 'Cargo.toml').Hash
$taskLockHash = (Get-FileHash -LiteralPath 'Cargo.lock').Hash
$taskResults = @()
$taskGates = if ($Build) { @('native-build') } else {
    @('native_aperture_tests','shell::native_composition::tests','inline_',
      'frame_keys','scrollbar_continuation','settled_timer',
      'terminal_document_turn','inline_component_station_fixture')
}
foreach ($taskGate in $taskGates) {
    $taskLabel = $taskGate.Replace('::','-')
    $taskLog = Join-Path $PSScriptRoot "published-$taskLabel.log"
    if ($Build) {
        if (Get-Process -Name turnstone -ErrorAction SilentlyContinue) {
            throw 'A native application still owns the executable; preserve it until its case finishes'
        }
        & cargo +1.99.0 build --bin turnstone --locked --offline -j1 *> $taskLog
        $taskCommand = 'cargo +1.99.0 build --bin turnstone --locked --offline -j1'
    } else {
        & cargo +1.99.0 test --lib $taskGate --locked --offline -j1 *> $taskLog
        $taskCommand = "cargo +1.99.0 test --lib $taskGate --locked --offline -j1"
    }
    $taskCode = $LASTEXITCODE
    $taskResults += @{ gate=$taskGate; exit=$taskCode; command=$taskCommand; log=[IO.Path]::GetFileName($taskLog) }
    $taskReceiptName = if ($Build) { 'published-native-build.json' } else { 'published-native-tests.json' }
    @{
        supplier='77fd235fe659c513dd0463ed57be32f02fb2e71a'
        genet='b6bfbeb556a6a17eddf8d5ad8254d3fcac95b084'
        netrender='6a24ae08f55129b474282b8e4911564f875ce5fd'
        local_path_override=$false; rust='1.99.0'; gates=$taskResults
        manifest_sha256=$taskManifestHash.ToLowerInvariant(); lock_sha256=$taskLockHash.ToLowerInvariant()
    } | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $PSScriptRoot $taskReceiptName)
    Write-Output "$taskGate exit $taskCode"
    if ($taskCode -ne 0) { exit $taskCode }
}
if ((Get-FileHash -LiteralPath 'Cargo.toml').Hash -ne $taskManifestHash -or
    (Get-FileHash -LiteralPath 'Cargo.lock').Hash -ne $taskLockHash) {
    throw 'Published consumer manifest or lock changed during qualification'
}
if ($Build) {
    @{ executable='C:/t/cargo-targets/turnstone/debug/turnstone.exe'
       sha256=(Get-FileHash -LiteralPath 'C:/t/cargo-targets/turnstone/debug/turnstone.exe').Hash.ToLowerInvariant()
       manifest_sha256=$taskManifestHash.ToLowerInvariant(); lock_sha256=$taskLockHash.ToLowerInvariant()
       finished_utc=[DateTime]::UtcNow.ToString('o')
    } | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot 'published-native-executable.json')
}
