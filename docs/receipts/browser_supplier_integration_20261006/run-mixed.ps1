param(
    [string] $Exe = 'C:/t/cargo-targets/turnstone/debug/turnstone.exe',
    [string] $ServoFixtureBase = 'http://127.0.0.1:43124',
    [string] $ScryFixtureBase = 'http://127.0.0.1:43123',
    [string] $CefPath = 'C:/Users/mark_/Code/cef-cache/wgpu-weld/151.3.24/cef_windows_x86_64',
    [string] $ServoProfile = 'supplier-b3-mixed',
    [ValidateRange(1,1800)][int] $TimeoutSeconds = 600
)
. (Join-Path $PSScriptRoot 'native-runner.ps1')
if ([string]::IsNullOrWhiteSpace($ServoProfile) -or $ServoProfile -ne $ServoProfile.Trim() -or
    $ServoProfile -match '[<>:"/\\|?*\x00-\x1f]' -or $ServoProfile.EndsWith('.')) {
    throw 'ServoProfile must be an explicit nonempty directory component'
}
$servoDirectory = Join-Path $PSScriptRoot "profile/servo-shared/$ServoProfile"
if (Test-Path -LiteralPath $servoDirectory) { throw "Fresh shared Servo profile already exists: $servoDirectory" }
$env:SERVO_FIXTURE_BASE=$ServoFixtureBase
$env:SCRY_FIXTURE_BASE=$ScryFixtureBase
$env:TURNSTONE_CEF_PATH=$CefPath
$env:TURNSTONE_SERVO_PROFILE=$ServoProfile
$env:TURNSTONE_SERVO_PROFILE_DIR=[System.IO.Path]::GetFullPath($servoDirectory)
$env:RUST_LOG='warn'
# Reuse the separately served, unchanged fixtures on their original origins.
# Freeze the --locked --features scry,weld,servo executable and matching ANGLE
# DLLs before launching. The common runner hashes the executable before launch,
# records owned root/CEF child loaded modules, and retains native exit/timeout
# guards. This gate starts Servo before WebView2/CEF in one process/device.
# Review current pixels and final-mixed-closed.surface-frames.json cached_frames
# separately; a RESULT ok sentinel alone does not establish either condition.
Invoke-SupplierReceipt -Name 'native-mixed' -Scenario 'scenarios/browser_trio_windows.scn' -Profile 'mixed' -PhasePrefix 'all3' -Exe $Exe -TimeoutSeconds $TimeoutSeconds
