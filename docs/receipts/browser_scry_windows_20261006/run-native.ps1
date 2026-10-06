param(
    [string] $Exe = 'C:/t/cargo-targets/turnstone/debug/turnstone.exe',
    [string] $FixtureBase = 'http://127.0.0.1:43123',
    [string] $CefPath = 'C:/Users/mark_/Code/cef-cache/wgpu-weld/151.3.24/cef_windows_x86_64'
)
$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
Set-Location $repository
$env:SCRY_FIXTURE_BASE = $FixtureBase
$env:SCRY_A_INPUT_X='240'; $env:SCRY_A_INPUT_Y='200'
$env:SCRY_A_BUTTON_X='180'; $env:SCRY_A_BUTTON_Y='266'
$env:SCRY_B_INPUT_X='755'; $env:SCRY_B_INPUT_Y='200'
$env:SCRY_B_BUTTON_X='695'; $env:SCRY_B_BUTTON_Y='266'
$env:RUST_LOG='warn'
$env:TURNSTONE_CEF_PATH=$CefPath
function Invoke-Receipt($Name, $Scenario, $Profile) {
    $out = Join-Path $PSScriptRoot $Name
    if (Test-Path $out) { throw "Receipt already exists: $out" }
    New-Item -ItemType Directory -Path $out | Out-Null
    $env:TURNSTONE_SCENARIO=(Resolve-Path $Scenario).Path
    $env:TURNSTONE_CAPTURE_DIR=$out
    $env:TURNSTONE_ROOT=Join-Path $PSScriptRoot "profile/$Profile"
    Write-Host "Running $Name"
    & $Exe > (Join-Path $out 'run.log') 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Native process failed: $Name" }
    $result=Get-Content (Join-Path $out 'scenario.done') -First 1
    Write-Host "${Name}: $result"
    if ($result -ne 'RESULT ok') { throw "Scenario failed: $Name" }
}
# Start the loopback fixture server separately, then build with --locked
# --features scry,weld. No diagnostic probe or local Cargo patch is used.
Invoke-Receipt 'native-production' 'scenarios/browser_scry_windows.scn' 'scry-production'
Invoke-Receipt 'native-production-restart' 'scenarios/fixtures/browser_scry/restart_verify.scn' 'scry-production'
Invoke-Receipt 'native-weld-final' 'scenarios/browser_weld_direct_windows.scn' 'weld-final'
