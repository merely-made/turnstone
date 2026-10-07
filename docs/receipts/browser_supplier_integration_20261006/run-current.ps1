param(
    [string] $Exe = 'C:/t/cargo-targets/turnstone/debug/turnstone.exe',
    [string] $FixtureBase = 'http://127.0.0.1:43123',
    [string] $CefPath = 'C:/Users/mark_/Code/cef-cache/wgpu-weld/151.3.24/cef_windows_x86_64',
    [ValidatePattern('^(?:[a-z0-9]+(?:-[a-z0-9]+)*)?$')][string] $PhasePrefix = '',
    [ValidateRange(1,1800)][int] $TimeoutSeconds = 600
)
. (Join-Path $PSScriptRoot 'native-runner.ps1')
$env:SCRY_FIXTURE_BASE = $FixtureBase
$env:SCRY_A_INPUT_X='240'; $env:SCRY_A_INPUT_Y='200'
$env:SCRY_A_BUTTON_X='180'; $env:SCRY_A_BUTTON_Y='266'
$env:SCRY_B_INPUT_X='755'; $env:SCRY_B_INPUT_Y='200'
$env:SCRY_B_BUTTON_X='695'; $env:SCRY_B_BUTTON_Y='266'
$env:RUST_LOG='warn'
$env:TURNSTONE_CEF_PATH=$CefPath
# Serve browser_scry fixtures separately on the original loopback origin.
# Freeze the --locked --features scry,weld executable before running these gates.
Invoke-SupplierReceipt -Name 'native-weld-input' -Scenario 'scenarios/browser_weld_direct_windows.scn' -Profile 'weld-input' -Exe $Exe -TimeoutSeconds $TimeoutSeconds -PhasePrefix $PhasePrefix
Invoke-SupplierReceipt -Name 'native-scry-input' -Scenario 'scenarios/browser_scry_windows.scn' -Profile 'scry-input' -Exe $Exe -TimeoutSeconds $TimeoutSeconds -PhasePrefix $PhasePrefix
Invoke-SupplierReceipt -Name 'native-scry-restart' -Scenario 'scenarios/fixtures/browser_scry/restart_verify.scn' -Profile 'scry-input' -Exe $Exe -TimeoutSeconds $TimeoutSeconds -PhasePrefix $PhasePrefix -Restart
