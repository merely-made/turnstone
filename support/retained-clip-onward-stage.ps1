# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

# Run one stage against a new, marker-owned synthetic custody fixture.
# Prepare packages and dr-c-receipt-host as documented in the receiver receipt.
param([Parameter(Mandatory)][ValidateSet('seed','export','receive','reopen','omitted')][string]$Stage)
$ErrorActionPreference='Stop'
$clipRepo=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$clipRuntime='C:/t/cargo-targets/turnstone/retained-clip-runtime'
if (-not (Test-Path "$clipRuntime/.retained-clip-generated") -or -not (Test-Path "$clipRuntime/broker/ready")) {
    throw 'Prepared synthetic receiver fixture is required'
}
$clipPackages=if (Test-Path "$clipRuntime/packages") { "$clipRuntime/packages" } else { "$clipRuntime/packages-held-away" }
$clipAddresses=Get-Content -Raw "$clipPackages/addresses.json" | ConvertFrom-Json
$clipScenarios=@{
    seed='retained_clip_seed.scn'; export='retained_clip_onward_export.scn';
    receive='retained_clip_onward_receive.scn'; reopen='retained_clip_onward_reopen.scn';
    omitted='retained_clip_onward_omitted.scn'
}
$clipCapture="$clipRuntime/onward-$Stage"
New-Item -ItemType Directory -Path $clipCapture | Out-Null
$clipValues=@{
    GRAPHSHELL_APP_ENDPOINT='\\.\pipe\turnstone-retained-clip-receipt';
    TURNSTONE_ROOT=$(if ($Stage -in @('seed','export')) { "$clipRuntime/receiver" } else { "$clipRuntime/receiver-second" });
    MERE_ROOT="$clipRuntime/shared"; TURNSTONE_OFFLINE='1';
    CLIP_PACKAGE_KNOT="$clipRuntime/packages/knot.mereclip";
    CLIP_PACKAGE_NOTE="$clipRuntime/packages/note.mereclip";
    CLIP_PACKAGE_VOICE="$clipRuntime/packages/voice.mereclip";
    CLIP_KNOT=$clipAddresses.knot; CLIP_NOTE=$clipAddresses.note; CLIP_VOICE=$clipAddresses.voice;
    CLIP_ONWARD_KNOT="$clipRuntime/onward/knot.mereclip";
    CLIP_ONWARD_VOICE="$clipRuntime/onward/voice.mereclip";
    TURNSTONE_SCENARIO=(Join-Path $clipRepo "scenarios/$($clipScenarios[$Stage])");
    TURNSTONE_CAPTURE_DIR=$clipCapture;
}
$clipPrevious=@{}
foreach ($clipKey in $clipValues.Keys) {
    $clipPrevious[$clipKey]=[Environment]::GetEnvironmentVariable($clipKey,'Process')
    [Environment]::SetEnvironmentVariable($clipKey,$clipValues[$clipKey],'Process')
}
$clipProcess=$null
try {
    $clipProcess=Start-Process C:/t/cargo-targets/turnstone/debug/turnstone.exe -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput "$clipCapture/stdout.log" -RedirectStandardError "$clipCapture/stderr.log"
    Set-Content "$clipCapture/pid" $clipProcess.Id
    $clipDeadline=(Get-Date).AddSeconds(90)
    while ((Get-Date) -lt $clipDeadline) {
        $clipProcess.Refresh()
        if ($clipProcess.HasExited) { break }
        Start-Sleep -Milliseconds 200
    }
    $clipProcess.Refresh()
    if (-not $clipProcess.HasExited) { throw 'Receiver scenario did not exit within its bounded gate' }
    $clipDone=Get-Content -Raw "$clipCapture/scenario.done"
    Write-Output $clipDone
    if (-not $clipDone.StartsWith('RESULT ok')) { throw "Stage $Stage failed" }
} finally {
    if ($clipProcess) {
        $clipProcess.Refresh()
        if (-not $clipProcess.HasExited) { Stop-Process -Id $clipProcess.Id }
    }
    foreach ($clipKey in $clipPrevious.Keys) {
        [Environment]::SetEnvironmentVariable($clipKey,$clipPrevious[$clipKey],'Process')
    }
}
