param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('servo','mixed','intl','input','permission')][string] $Kind,
    [Parameter(Mandatory=$true)]
    [ValidatePattern('^[a-z0-9]+(?:-[a-z0-9]+)*$')][string] $RunLabel,
    [string] $FingerprintLabel = 'all3-current',
    [string] $Exe = 'C:/t/cargo-targets/turnstone/debug/turnstone.exe',
    [string] $CefPath = 'C:/Users/mark_/Code/cef-cache/wgpu-weld/154.0.34/cef_windows_x86_64',
    [string] $ServoFixtureBase = 'http://127.0.0.1:43124',
    [string] $ScryFixtureBase = 'http://127.0.0.1:43123',
    [ValidateRange(1,1800)][int] $TimeoutSeconds = 600
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native-runner.ps1')
# Each run keeps its own captures and private profile. Historical scripts and
# receipts keep their old SDK identities. Fixture servers are started separately.
python -X utf8 (Join-Path $PSScriptRoot 'fingerprint.py') --label $FingerprintLabel --features scry,weld,servo --exe $Exe --verify
if ($LASTEXITCODE -ne 0) { throw 'Current upstream executable/source fingerprint refused' }
$env:TURNSTONE_CEF_PATH = $CefPath
$env:SERVO_FIXTURE_BASE = $ServoFixtureBase
$env:SCRY_FIXTURE_BASE = $ScryFixtureBase
$env:RUST_LOG = 'warn'
if ($Kind -eq 'input') {
    & (Join-Path $PSScriptRoot 'run-current.ps1') -Exe $Exe -FixtureBase $ScryFixtureBase -CefPath $CefPath -PhasePrefix $RunLabel -TimeoutSeconds $TimeoutSeconds
} elseif ($Kind -eq 'permission') {
    & (Join-Path $PSScriptRoot 'run-permission.ps1') -Exe $Exe -CefPath $CefPath -PhasePrefix $RunLabel -TimeoutSeconds $TimeoutSeconds
} else {
    $servoDirectory = Join-Path $PSScriptRoot "profile/servo-shared/$RunLabel"
    if (Test-Path -LiteralPath $servoDirectory) { throw "Fresh Servo profile already exists: $servoDirectory" }
    $env:TURNSTONE_SERVO_PROFILE = $RunLabel
    $env:TURNSTONE_SERVO_PROFILE_DIR = [System.IO.Path]::GetFullPath($servoDirectory)
    $scenario = switch ($Kind) {
        'servo' { 'scenarios/browser_servo_windows.scn' }
        'intl' { 'scenarios/browser_servo_intl_windows.scn' }
        default { 'scenarios/browser_trio_windows.scn' }
    }
    Invoke-SupplierReceipt -Name "native-$Kind" -Scenario $scenario -Profile $Kind -Exe $Exe -PhasePrefix $RunLabel -TimeoutSeconds $TimeoutSeconds
}
