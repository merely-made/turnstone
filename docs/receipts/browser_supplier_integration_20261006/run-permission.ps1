param(
    [string] $Exe = 'C:/t/cargo-targets/turnstone/debug/turnstone.exe',
    [string] $CefPath = 'C:/Users/mark_/Code/cef-cache/wgpu-weld/151.3.24/cef_windows_x86_64',
    [ValidatePattern('^(?:[a-z0-9]+(?:-[a-z0-9]+)*)?$')][string] $PhasePrefix = '',
    [ValidateRange(1,1800)][int] $TimeoutSeconds = 600
)
. (Join-Path $PSScriptRoot 'native-runner.ps1')
$env:TURNSTONE_CEF_PATH=$CefPath
$env:RUST_LOG='warn'
# Start browser_decisions_server.ps1 separately on the original port 43107.
Invoke-SupplierReceipt -Name 'native-weld-permission' -Scenario 'scenarios/browser_weld_permission_direct_windows.scn' -Profile 'weld-permission' -Exe $Exe -TimeoutSeconds $TimeoutSeconds -PhasePrefix $PhasePrefix
