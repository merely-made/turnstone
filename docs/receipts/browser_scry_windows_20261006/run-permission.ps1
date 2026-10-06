param(
    [string] $Exe = 'C:/t/cargo-targets/turnstone/debug/turnstone.exe',
    [string] $CefPath = 'C:/Users/mark_/Code/cef-cache/wgpu-weld/151.3.24/cef_windows_x86_64'
)
$ErrorActionPreference='Stop'
$repository=(Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
Set-Location $repository
$out=Join-Path $PSScriptRoot 'native-weld-permission-final'
if (Test-Path $out) { throw "Receipt already exists: $out" }
New-Item -ItemType Directory -Path $out | Out-Null
$env:TURNSTONE_SCENARIO=Join-Path $repository 'scenarios/browser_weld_permission_direct_windows.scn'
$env:TURNSTONE_CAPTURE_DIR=$out
$env:TURNSTONE_ROOT=Join-Path $PSScriptRoot 'profile/weld-permission-final-input'
$env:TURNSTONE_CEF_PATH=$CefPath
$env:RUST_LOG='warn'
# Start browser_decisions_server.ps1 separately on port 43107. This script
# invokes ordinary retained chrome clicks and checks a real CEF callback.
# Windows PowerShell 5 turns redirected native stderr into ErrorRecords.
    # Keep routine producer diagnostics non-terminating; gate on exit/result.
    $ErrorActionPreference='Continue'
    & $Exe > (Join-Path $out 'run.log') 2>&1
    $nativeExit=$LASTEXITCODE
    $ErrorActionPreference='Stop'
if ($nativeExit -ne 0) { throw 'Native permission process failed' }
$result=Get-Content (Join-Path $out 'scenario.done') -First 1
Write-Host "Permission: $result"
if ($result -ne 'RESULT ok') { throw 'Native permission scenario failed' }
