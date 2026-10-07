param(
    [string] $Exe = 'C:/t/cargo-targets/turnstone/debug/turnstone.exe',
    [string] $FixtureBase = 'http://127.0.0.1:43124',
    [string] $ServoProfile = 'supplier-b3-shared',
    [ValidatePattern('^[a-z0-9]+(?:-[a-z0-9]+)*$')][string] $ApplicationProfile = 'servo-app',
    [ValidateRange(1,1800)][int] $TimeoutSeconds = 600
)
. (Join-Path $PSScriptRoot 'native-runner.ps1')
if ([string]::IsNullOrWhiteSpace($ServoProfile) -or $ServoProfile -ne $ServoProfile.Trim() -or
    $ServoProfile -match '[<>:"/\\|?*\x00-\x1f]' -or $ServoProfile.EndsWith('.')) {
    throw 'ServoProfile must be an explicit nonempty directory component'
}
$servoDirectory = Join-Path $PSScriptRoot "profile/servo-shared/$ServoProfile"
if (Test-Path -LiteralPath $servoDirectory) { throw "Fresh shared Servo profile already exists: $servoDirectory" }
$env:SERVO_FIXTURE_BASE=$FixtureBase
$env:TURNSTONE_SERVO_PROFILE=$ServoProfile
$env:TURNSTONE_SERVO_PROFILE_DIR=[System.IO.Path]::GetFullPath($servoDirectory)
$env:RUST_LOG='warn'
# Serve browser_servo separately on 127.0.0.1:43124. Freeze the executable
# built with --locked --features scry,weld,servo before this native gate.
Invoke-SupplierReceipt -Name 'native-servo' -Scenario 'scenarios/browser_servo_windows.scn' -Profile $ApplicationProfile -Exe $Exe -TimeoutSeconds $TimeoutSeconds
