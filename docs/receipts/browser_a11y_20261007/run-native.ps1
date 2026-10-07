# Copyright 2026 Mark Alan Boykin. SPDX-License-Identifier: MPL-2.0
param(
    [string] $Exe='C:/t/cargo-targets/turnstone/debug/turnstone.exe',
    [string] $FixtureBase='http://127.0.0.1:43124',
    [ValidateSet('document-root','upstream')][string] $FocusPolicy='document-root',
    [string] $Scenario='scenarios/browser_servo_a11y_windows.scn',
    [ValidatePattern('^[a-z0-9]+(?:-[a-z0-9]+)*$')][string] $PhasePrefix='a11y',
    [ValidateRange(1,1800)][int] $TimeoutSeconds=600
)
$ErrorActionPreference='Stop'
$legacyRoot=Join-Path (Split-Path $PSScriptRoot -Parent) 'browser_supplier_integration_20261006'
. (Join-Path $legacyRoot 'native-runner.ps1')
$out=Join-Path $legacyRoot "native-$PhasePrefix-servo"
$servoDirectory="C:/t/cargo-targets/turnstone/runtime/servo-$PhasePrefix"
if (Test-Path -LiteralPath $servoDirectory) { throw "Preserving existing Servo profile: $servoDirectory" }
if (Test-Path -LiteralPath $out) { throw "Preserving existing native receipt: $out" }
$scenarioPath=(Resolve-Path -LiteralPath (Join-Path $receiptRepository $Scenario)).Path
$scenarioHash=(Get-FileHash -LiteralPath $scenarioPath -Algorithm SHA256).Hash.ToLowerInvariant()
$fixturePath=Join-Path $receiptRepository 'scenarios/fixtures/browser_servo/a11y.html'
$fixtureBytes=[IO.File]::ReadAllBytes($fixturePath)
$servedFixture=(Invoke-WebRequest -Uri "$FixtureBase/a11y.html?page=A" -TimeoutSec 15).RawContentStream.ToArray()
$fixtureHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($fixtureBytes)).ToLowerInvariant()
$servedHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($servedFixture)).ToLowerInvariant()
if ($fixtureHash -ne $servedHash) { throw 'Fixture server does not serve the admitted public source bytes' }
$cssPath=Join-Path $receiptRepository 'scenarios/fixtures/browser_servo/fixture.css'
$cssBytes=[IO.File]::ReadAllBytes($cssPath)
$servedCss=(Invoke-WebRequest -Uri "$FixtureBase/fixture.css" -TimeoutSec 15).RawContentStream.ToArray()
$cssHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($cssBytes)).ToLowerInvariant()
$servedCssHash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($servedCss)).ToLowerInvariant()
if ($cssHash -ne $servedCssHash) { throw 'Fixture server does not serve the admitted fixture CSS bytes' }
$env:SERVO_FIXTURE_BASE=$FixtureBase
$env:TURNSTONE_SERVO_PROFILE="servo-$PhasePrefix"
$env:TURNSTONE_SERVO_PROFILE_DIR=$servoDirectory
$env:TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT='1'
$env:TURNSTONE_SERVO_A11Y_FOCUS=$FocusPolicy
$env:RUST_LOG='warn,turnstone::shell::foreign_a11y=debug,turnstone::servo::a11y=debug'
$sourceInputs=[ordered]@{
    scope='Source-bound real Servo composition and Windows UIA public-fixture gate; human and other-platform AT remain unqualified'
    recorded_utc=[DateTime]::UtcNow.ToString('o'); executable=(Get-FileHash -LiteralPath $Exe -Algorithm SHA256).Hash.ToLowerInvariant()
    fixture_sha256=$fixtureHash; served_fixture_sha256=$servedHash
    fixture_css_sha256=$cssHash; served_fixture_css_sha256=$servedCssHash
    scenario=$Scenario; scenario_sha256=$scenarioHash
    source_head=(git -C $receiptRepository rev-parse HEAD)
    source_status=@(git -C $receiptRepository status --short)
    public_fixture_text=$true; servo_profile_directory=$servoDirectory
    servo_focus_policy=$env:TURNSTONE_SERVO_A11Y_FOCUS
    inputs=@(git -C $receiptRepository ls-files src Cargo.toml Cargo.lock rust-toolchain.toml scenarios/browser_servo_a11y_windows.scn scenarios/fixtures/browser_servo/a11y.html | ForEach-Object {
        $path=Join-Path $receiptRepository $_
        [ordered]@{path=$_; sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
    })
}
# Include new source and every helper/style input, whether tracked or not.
foreach ($relative in @(
    'src/foreign_a11y.rs','src/shell/foreign_a11y.rs',
    'scenarios/browser_servo_a11y_windows.scn','scenarios/fixtures/browser_servo/a11y.html',
    'scenarios/fixtures/browser_servo/fixture.css',
    'docs/receipts/browser_a11y_20261007/run-native.ps1',
    'docs/receipts/browser_a11y_20261007/observe-owned-uia.ps1',
    'docs/receipts/browser_supplier_integration_20261006/native-runner.ps1',
    $Scenario
)) {
    if (-not ($sourceInputs.inputs.path -contains $relative)) {
        $sourceInputs.inputs += [ordered]@{path=$relative; sha256=(Get-FileHash -LiteralPath (Join-Path $receiptRepository $relative) -Algorithm SHA256).Hash.ToLowerInvariant()}
    }
}
$inputPath=Join-Path $PSScriptRoot "native-inputs-$PhasePrefix.json"
if (Test-Path -LiteralPath $inputPath) { throw "Preserving existing source receipt: $inputPath" }
[IO.File]::WriteAllText($inputPath,($sourceInputs | ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
$observer=Start-Job -FilePath (Join-Path $PSScriptRoot 'observe-owned-uia.ps1') -ArgumentList $out,90
try {
    Invoke-SupplierReceipt -Name 'native-servo' -PhasePrefix $PhasePrefix -Scenario $Scenario -Profile 'servo-app' -Exe $Exe -TimeoutSeconds $TimeoutSeconds
    $observer | Wait-Job -Timeout 10 | Out-Null
    if ($observer.State -ne 'Completed') { throw "UIA observer did not complete: $($observer.State)" }
    $observer | Receive-Job -ErrorAction Stop
    $uia=Get-Content -LiteralPath (Join-Path $out 'uia-tree.json') -Raw | ConvertFrom-Json
    if (-not $uia.passed) { throw 'Native scenario completed but Windows UIA traversal failed' }
} finally {
    if ($observer.State -in @('Running','NotStarted')) { $observer | Stop-Job }
    $observer | Remove-Job
}
