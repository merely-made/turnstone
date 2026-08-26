param(
    [Parameter(Mandatory = $true)]
    [string] $TurnstoneBin,

    [string] $OutputRoot = (Join-Path (
        [System.IO.Path]::GetTempPath()
    ) ("turnstone-sky-acceptance-" + (Get-Date -Format "yyyyMMdd-HHmmss")))
)

$ErrorActionPreference = "Stop"

$scenarioRoot = $PSScriptRoot
$repositoryRoot = Split-Path -Parent $scenarioRoot
$scenario = Join-Path $scenarioRoot "sky_home.scn"
$binaryInput = if ([System.IO.Path]::IsPathRooted($TurnstoneBin)) {
    $TurnstoneBin
}
else {
    Join-Path $repositoryRoot $TurnstoneBin
}
$binary = [System.IO.Path]::GetFullPath($binaryInput)
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "Turnstone binary does not exist: $binary"
}
if (-not (Test-Path -LiteralPath $scenario -PathType Leaf)) {
    throw "Sky scenario does not exist: $scenario"
}

[System.IO.Directory]::CreateDirectory($OutputRoot) | Out-Null
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
$profileRoot = Join-Path $OutputRoot "profile"
$captureRoot = Join-Path $OutputRoot "capture"
foreach ($freshRoot in @($profileRoot, $captureRoot)) {
    if (Test-Path -LiteralPath $freshRoot) {
        throw "Sky acceptance requires a fresh path: $freshRoot"
    }
}
[System.IO.Directory]::CreateDirectory($profileRoot) | Out-Null
[System.IO.Directory]::CreateDirectory($captureRoot) | Out-Null

$scenarioDone = Join-Path $captureRoot "scenario.done"
$capture = Join-Path $captureRoot "sky_home.png"
foreach ($receipt in @($scenarioDone, $capture)) {
    if (Test-Path -LiteralPath $receipt) {
        throw "refusing to overwrite an existing receipt: $receipt"
    }
}

$previousRoot = [Environment]::GetEnvironmentVariable("TURNSTONE_ROOT", "Process")
$previousScenario = [Environment]::GetEnvironmentVariable("TURNSTONE_SCENARIO", "Process")
$previousCapture = [Environment]::GetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", "Process")
try {
    [Environment]::SetEnvironmentVariable("TURNSTONE_ROOT", $profileRoot, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_SCENARIO", $scenario, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", $captureRoot, "Process")

    Push-Location $repositoryRoot
    try {
        & $binary
        $appExit = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
}
finally {
    [Environment]::SetEnvironmentVariable("TURNSTONE_ROOT", $previousRoot, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_SCENARIO", $previousScenario, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", $previousCapture, "Process")
}

if (-not (Test-Path -LiteralPath $scenarioDone -PathType Leaf)) {
    throw "Sky scenario produced no scenario.done"
}
$scenarioResult = Get-Content -LiteralPath $scenarioDone
if ($scenarioResult.Count -eq 0 -or $scenarioResult[0] -ne "RESULT ok") {
    throw "Sky scenario failed:`n$($scenarioResult -join "`n")"
}
if ($appExit -ne 0) {
    throw "Sky scenario returned process exit $appExit after RESULT ok"
}
if (-not (Test-Path -LiteralPath $capture -PathType Leaf)) {
    throw "Sky scenario reported success without sky_home.png"
}

[byte[]] $png = [System.IO.File]::ReadAllBytes($capture)
[byte[]] $signature = @(137, 80, 78, 71, 13, 10, 26, 10)
if ($png.Length -lt 24 -or (($png[0..7] -join ',') -ne ($signature -join ','))) {
    throw "Sky capture is not a decodable PNG header"
}
$chunkType = [System.Text.Encoding]::ASCII.GetString($png, 12, 4)
if ($chunkType -ne "IHDR") {
    throw "Sky capture has no leading IHDR chunk"
}
$width = [System.Net.IPAddress]::NetworkToHostOrder([System.BitConverter]::ToInt32($png, 16))
$height = [System.Net.IPAddress]::NetworkToHostOrder([System.BitConverter]::ToInt32($png, 20))
if ($width -le 0 -or $height -le 0) {
    throw "Sky capture has invalid dimensions ${width}x${height}"
}

$acceptance = Join-Path $OutputRoot "acceptance.done"
[System.IO.File]::WriteAllLines(
    $acceptance,
    @(
        "RESULT ok",
        "scenario=$scenario",
        "capture=$capture",
        "bytes=$($png.Length)",
        "dimensions=${width}x${height}"
    )
)
Write-Output $acceptance
