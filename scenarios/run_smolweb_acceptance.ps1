param(
    [Parameter(Mandatory = $true)]
    [string] $TurnstoneBin,

    [ValidateSet("gemini-browse", "gemini-input", "gemini-inline-image", "gemini-download", "gemini-streaming", "gemini-typography", "titan-mutation", "spartan-mutation")]
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
        Name = "gemini-inline-image"
        Scenario = "smolweb_inline_image.scn"
        Server = "GeminiImage"
        Port = 19652
    },
    [pscustomobject]@{
        Name = "gemini-download"
        Scenario = "smolweb_download.scn"
        Server = "GeminiDownload"
        Port = 19653
    },
    [pscustomobject]@{
        Name = "gemini-streaming"
        Scenario = "smolweb_streaming.scn"
        Server = "GeminiStreaming"
        Port = 19654
        ReleaseCapture = "01_streaming_prefix.png"
    },
    [pscustomobject]@{
        Name = "gemini-typography"
        Scenario = "smolweb_typography.scn"
        Server = "GeminiTypography"
        Port = 19655
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
$previousDownload = [Environment]::GetEnvironmentVariable("TURNSTONE_DOWNLOAD_DIR", "Process")
$results = [System.Collections.Generic.List[string]]::new()

try {
    foreach ($case in $cases) {
        $caseRoot = Join-Path $OutputRoot $case.Name
        $profileRoot = Join-Path $caseRoot "profile"
        $captureRoot = Join-Path $caseRoot "capture"
        $downloadRoot = Join-Path $caseRoot "downloads"
        [System.IO.Directory]::CreateDirectory($profileRoot) | Out-Null
        [System.IO.Directory]::CreateDirectory($captureRoot) | Out-Null
        [System.IO.Directory]::CreateDirectory($downloadRoot) | Out-Null

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
                if ($null -ne $case.ReleaseCapture) {
                    $serverStart.ArgumentList += @(
                        "-ReleasePath", (Join-Path $captureRoot $case.ReleaseCapture)
                    )
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
            [Environment]::SetEnvironmentVariable("TURNSTONE_DOWNLOAD_DIR", $downloadRoot, "Process")

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

            if ($case.Name -eq "gemini-download") {
                $downloads = @(Get-ChildItem -LiteralPath $downloadRoot -File)
                if ($downloads.Count -ne 1 -or $downloads[0].Name -ne "archive.bin") {
                    throw "download custody wrote unexpected destinations: $($downloads.Name -join ', ')"
                }
                [byte[]] $expected = @(0, 1, 2, 255, 84, 117, 114, 110, 115, 116, 111, 110, 101)
                [byte[]] $actual = [System.IO.File]::ReadAllBytes($downloads[0].FullName)
                if (($expected -join ',') -ne ($actual -join ',')) {
                    throw "downloaded bytes did not match the Gemini response"
                }
                $representationStores = @(
                    Get-ChildItem -LiteralPath $profileRoot -Filter "representations.redb" -File -Recurse
                )
                $graphs = @(Get-ChildItem -LiteralPath $profileRoot -Filter "graph.json" -File -Recurse)
                $facets = @(Get-ChildItem -LiteralPath $profileRoot -Filter "facets.json" -File -Recurse)
                if ($representationStores.Count -ne 1) {
                    throw "download custody did not leave one session representation store"
                }
                if ($graphs.Count -ne 1 -or -not (Select-String -LiteralPath $graphs[0].FullName -SimpleMatch '"content_hash"')) {
                    throw "download graph node did not persist a content hash"
                }
                if ($facets.Count -ne 1 -or
                    -not (Select-String -LiteralPath $facets[0].FullName -SimpleMatch 'download.response') -or
                    -not (Select-String -LiteralPath $facets[0].FullName -SimpleMatch 'completed')) {
                    throw "download custody facet did not persist its completed state"
                }
                $hash = (Get-FileHash -LiteralPath $downloads[0].FullName -Algorithm SHA256).Hash
                [System.IO.File]::WriteAllLines(
                    (Join-Path $caseRoot "custody.done"),
                    @(
                        "RESULT ok",
                        "destination=$($downloads[0].FullName)",
                        "bytes=$($actual.Length)",
                        "sha256=$hash",
                        "representation-store=$($representationStores[0].FullName)",
                        "graph-content-hash=true",
                        "facet-status=completed"
                    )
                )
            }

            if ($case.Name -eq "gemini-streaming") {
                foreach ($capture in @("01_streaming_prefix.png", "02_streaming_complete.png")) {
                    if (-not (Test-Path -LiteralPath (Join-Path $captureRoot $capture) -PathType Leaf)) {
                        throw "streaming acceptance omitted capture $capture"
                    }
                }
                if (-not (Select-String -LiteralPath $serverDone -SimpleMatch 'release-observed-before-tail=True')) {
                    throw "streaming server did not prove the prefix capture preceded its tail"
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
    [Environment]::SetEnvironmentVariable("TURNSTONE_DOWNLOAD_DIR", $previousDownload, "Process")
}

$summary = Join-Path $OutputRoot "acceptance.done"
[System.IO.File]::WriteAllLines($summary, @("RESULT ok") + $results)
Get-Content -LiteralPath $summary
Write-Host "Receipts: $OutputRoot"
