param(
    [Parameter(Mandatory = $true)]
    [string] $TurnstoneBin,

    [ValidateSet("gemini-browse", "gemini-input", "titan-mutation", "spartan-mutation")]
    [string[]] $Only = @(),

    [string] $OutputRoot = (Join-Path (
        [System.IO.Path]::GetTempPath()
    ) ("turnstone-smolweb-acceptance-" + (Get-Date -Format "yyyyMMdd-HHmmss")))
)

$ErrorActionPreference = "Stop"

$scenarioRoot = $PSScriptRoot
$repositoryRoot = Split-Path -Parent $scenarioRoot
$fixture = Join-Path $scenarioRoot "fixtures\smolweb_acceptance_server.ps1"
$binary = [System.IO.Path]::GetFullPath($TurnstoneBin)
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "Turnstone binary does not exist: $binary"
}
[System.IO.Directory]::CreateDirectory($OutputRoot) | Out-Null
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)

$cases = @(
    [pscustomobject]@{
        Name = "gemini-browse"
        Scenario = "smolweb_gemini.scn"
        Server = $null
        Port = 0
    },
    [pscustomobject]@{
        Name = "gemini-input"
        Scenario = "smolweb_input.scn"
        Server = $null
        Port = 0
    },
    [pscustomobject]@{
        Name = "titan-mutation"
        Scenario = "smolweb_titan.scn"
        Server = "Titan"
        Port = 19651
    },
    [pscustomobject]@{
        Name = "spartan-mutation"
        Scenario = "smolweb_spartan.scn"
        Server = "Spartan"
        Port = 30001
    }
)
if ($Only.Count -gt 0) {
    $cases = @($cases | Where-Object { $_.Name -in $Only })
}

$previousRoot = [Environment]::GetEnvironmentVariable("TURNSTONE_ROOT", "Process")
$previousScenario = [Environment]::GetEnvironmentVariable("TURNSTONE_SCENARIO", "Process")
$previousCapture = [Environment]::GetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", "Process")
$results = [System.Collections.Generic.List[string]]::new()

try {
    foreach ($case in $cases) {
        $caseRoot = Join-Path $OutputRoot $case.Name
        $profileRoot = Join-Path $caseRoot "profile"
        $captureRoot = Join-Path $caseRoot "capture"
        [System.IO.Directory]::CreateDirectory($profileRoot) | Out-Null
        [System.IO.Directory]::CreateDirectory($captureRoot) | Out-Null

        $scenarioDone = Join-Path $captureRoot "scenario.done"
        if (Test-Path -LiteralPath $scenarioDone) {
            throw "refusing to overwrite an existing receipt: $scenarioDone"
        }

        $serverProcess = $null
        $serverDone = Join-Path $caseRoot "server.done"
        try {
            if ($null -ne $case.Server) {
                $ready = Join-Path $caseRoot "server.ready"
                $serverStart = @{
                    FilePath = (Join-Path $PSHOME "pwsh.exe")
                    ArgumentList = @(
                        "-NoProfile",
                        "-File", $fixture,
                        "-Mode", $case.Server,
                        "-Port", $case.Port,
                        "-ReadyPath", $ready,
                        "-ReceiptPath", $serverDone
                    )
                    WindowStyle = "Hidden"
                    PassThru = $true
                }
                $serverProcess = Start-Process @serverStart
                for ($attempt = 0; $attempt -lt 200 -and -not (Test-Path -LiteralPath $ready); $attempt++) {
                    if ($serverProcess.HasExited) {
                        throw "$($case.Server) fixture exited before becoming ready"
                    }
                    Start-Sleep -Milliseconds 50
                }
                if (-not (Test-Path -LiteralPath $ready)) {
                    throw "$($case.Server) fixture did not become ready"
                }
            }

            [Environment]::SetEnvironmentVariable("TURNSTONE_ROOT", $profileRoot, "Process")
            [Environment]::SetEnvironmentVariable(
                "TURNSTONE_SCENARIO",
                (Join-Path $scenarioRoot $case.Scenario),
                "Process"
            )
            [Environment]::SetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", $captureRoot, "Process")

            Write-Host "RUN $($case.Name)"
            Push-Location $repositoryRoot
            try {
                & $binary
                $appExit = $LASTEXITCODE
            }
            finally {
                Pop-Location
            }

            if (-not (Test-Path -LiteralPath $scenarioDone)) {
                throw "$($case.Name) produced no scenario.done"
            }
            $scenarioResult = Get-Content -LiteralPath $scenarioDone
            if ($scenarioResult[0] -ne "RESULT ok") {
                throw "$($case.Name) failed:`n$($scenarioResult -join "`n")"
            }
            if ($appExit -ne 0) {
                throw "$($case.Name) returned process exit $appExit after RESULT ok"
            }

            if ($null -ne $serverProcess) {
                if (-not $serverProcess.WaitForExit(10000)) {
                    throw "$($case.Server) fixture did not finish after the app exited"
                }
                if ($serverProcess.ExitCode -ne 0) {
                    throw "$($case.Server) fixture exited $($serverProcess.ExitCode)"
                }
            }

            if ($null -ne $case.Server) {
                if (-not (Test-Path -LiteralPath $serverDone)) {
                    throw "$($case.Server) fixture produced no server.done"
                }
                $serverResult = Get-Content -LiteralPath $serverDone
                if ($serverResult[0] -ne "RESULT ok") {
                    throw "$($case.Server) wire receipt failed:`n$($serverResult -join "`n")"
                }
            }

            $results.Add("RESULT ok $($case.Name)")
        }
        finally {
            if ($null -ne $serverProcess -and -not $serverProcess.HasExited) {
                Stop-Process -Id $serverProcess.Id -Force
            }
        }
    }
}
finally {
    [Environment]::SetEnvironmentVariable("TURNSTONE_ROOT", $previousRoot, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_SCENARIO", $previousScenario, "Process")
    [Environment]::SetEnvironmentVariable("TURNSTONE_CAPTURE_DIR", $previousCapture, "Process")
}

$summary = Join-Path $OutputRoot "acceptance.done"
[System.IO.File]::WriteAllLines($summary, @("RESULT ok") + $results)
Get-Content -LiteralPath $summary
Write-Host "Receipts: $OutputRoot"
