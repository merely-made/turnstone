# Helpers for the exact-source native supplier qualification. Does not build.
$ErrorActionPreference = 'Stop'
$receiptRepository = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path

function Protect-SupplierObservation {
    param([string] $Text)
    # Never retain process command lines. Also redact credential-shaped values
    # if an observer error or a file path happens to contain one.
    $safe = $Text -replace '(?i)(https?://)[^/@\s]+@', '$1[redacted]@'
    return $safe -replace '(?i)((?:password|passwd|token|secret|authorization|credential|api[-_]?key|sig|signature|x-amz-signature)\s*[=:]\s*)[^\s&;]+', '$1[redacted]'
}

function Observe-SupplierModules {
    param($OwnedProcess, $ObservedPaths, $Limitations, $ProcessEvidence)
    try {
        $OwnedProcess.Refresh()
        # The caller has verified this live process handle's owned lineage.
        foreach ($module in $OwnedProcess.Modules) {
            if ($module.ModuleName -notmatch '^(libEGL|libGLESv2|libcef|WebView2Loader|EmbeddedBrowserWebView|msedge|d3d11|d3d12|D3D12Core|dxgi|dxcompiler|dxil|vulkan-1|vk_swiftshader|opengl32)\.dll$' -and
                $module.ModuleName -notmatch '(wgpu|graft|scry|weld).*\.dll$') { continue }
            $path = $module.FileName
            $moduleKey = "$($OwnedProcess.Id)|$($ProcessEvidence.created_utc)|$path"
            if ($ObservedPaths.ContainsKey($moduleKey)) { continue }
            $entry = [ordered]@{
                pid=$OwnedProcess.Id; parent_pid=$ProcessEvidence.parent_pid;
                process_created_utc=$ProcessEvidence.created_utc;
                command_line_flags=$ProcessEvidence.command_line_flags;
                module=$module.ModuleName; path=(Protect-SupplierObservation $path); sha256=$null;
                file_version=$null; product_version=$null;
                first_observed_utc=[DateTime]::UtcNow.ToString('o'); hash_limitation=$null
            }
            try {
                $entry.sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
                $entry.file_version = Protect-SupplierObservation $module.FileVersionInfo.FileVersion
                $entry.product_version = Protect-SupplierObservation $module.FileVersionInfo.ProductVersion
            } catch {
                $entry.hash_limitation = Protect-SupplierObservation $_.Exception.Message
                [void] $Limitations.Add((Protect-SupplierObservation "Observed module could not be hashed: $path; $($_.Exception.Message)"))
            }
            # Distinguish the same DLL loaded by different owned processes.
            $ObservedPaths[$moduleKey] = $entry
        }
    } catch {
        # Access restrictions or normal process-exit races are observer limits.
        # They cannot establish that an expected module was absent.
        [void] $Limitations.Add((Protect-SupplierObservation "Owned-process module observation unavailable: $($_.Exception.Message)"))
    }
}

function Observe-SupplierChildren {
    param($RootProcess, [DateTime] $RootStartedUtc, $ObservedPaths, $ObservedProcesses, $Limitations)
    # This is read-only. No child is adopted for termination or gate acceptance.
    # Traverse only live parents already connected to this exact owned root.
    $pending = New-Object 'System.Collections.Generic.Queue[object]'
    $pending.Enqueue($ObservedProcesses["$($RootProcess.Id)"])
    $visited = New-Object 'System.Collections.Generic.HashSet[int]'
    $processLimit = 64
    while ($pending.Count -gt 0 -and $visited.Count -lt $processLimit) {
        $parent = $pending.Dequeue()
        $parentId = [int] $parent.pid
        if (-not $visited.Add($parentId)) { continue }
        $parentHandle = $null
        try {
            $RootProcess.Refresh()
            if ($RootProcess.HasExited) { break }
            if ($parentId -eq $RootProcess.Id) {
                $parentHandle = $RootProcess
            } else {
                $parentHandle = [System.Diagnostics.Process]::GetProcessById($parentId)
            }
            $parentHandle.Refresh()
            if ($parentHandle.HasExited -or
                $parentHandle.StartTime.ToUniversalTime().ToString('o') -ne $parent.created_utc) {
                [void] $Limitations.Add("Owned lineage parent exited or changed identity: PID $parentId")
                continue
            }
            $candidates = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $parentId" -OperationTimeoutSec 2 -ErrorAction Stop)
            foreach ($candidate in $candidates) {
                if ($visited.Count + $pending.Count -ge $processLimit) {
                    [void] $Limitations.Add('Owned child observation reached the 64-process snapshot limit')
                    break
                }
                # Keep only the CEF role marker, never URLs, credentials or the
                # remainder of the command line. The marker alone is not proof
                # of a loaded CEF DLL; the module receipt supplies that evidence.
                $commandLine = [string] $candidate.CommandLine
                if ($commandLine -notmatch '(?i)(?:^|\s)--type(?:=|\s+)([a-z0-9_-]{1,64})(?=\s|$)') { continue }
                $role = $Matches[1].ToLowerInvariant()
                $childId = [int] $candidate.ProcessId
                $childHandle = $null
                try {
                    if ($null -eq $candidate.CreationDate) {
                        [void] $Limitations.Add("Owned child creation time unavailable: PID $childId")
                        continue
                    }
                    $candidateCreatedUtc = ([DateTime] $candidate.CreationDate).ToUniversalTime()
                    $parentCreatedUtc = [DateTime]::Parse($parent.created_utc).ToUniversalTime()
                    if ($candidateCreatedUtc -lt $RootStartedUtc -or $candidateCreatedUtc -lt $parentCreatedUtc) { continue }
                    $childHandle = [System.Diagnostics.Process]::GetProcessById($childId)
                    $childHandle.Refresh()
                    $childCreatedUtc = $childHandle.StartTime.ToUniversalTime()
                    # CIM DateTime truncates sub-millisecond process time. Require
                    # the same creation instant within that representation limit.
                    if ($childHandle.HasExited -or
                        [Math]::Abs(($childCreatedUtc - $candidateCreatedUtc).TotalMilliseconds) -ge 1 -or
                        $childCreatedUtc -lt $RootStartedUtc -or $childCreatedUtc -lt $parentCreatedUtc) {
                        [void] $Limitations.Add("Owned child exited or changed identity during observation: PID $childId")
                        continue
                    }
                    $RootProcess.Refresh()
                    $parentHandle.Refresh()
                    if ($RootProcess.HasExited -or $parentHandle.HasExited -or
                        $parentHandle.StartTime.ToUniversalTime().ToString('o') -ne $parent.created_utc) {
                        [void] $Limitations.Add("Owned child parent no longer live at module observation: PID $parentId")
                        continue
                    }
                    $evidence = [ordered]@{
                        pid=$childId; parent_pid=$parentId; created_utc=$childCreatedUtc.ToString('o');
                        command_line_flags=@("--type=$role"); lineage='verified live parent chain to owned root';
                        first_observed_utc=[DateTime]::UtcNow.ToString('o')
                    }
                    $processKey = "$childId"
                    if ($ObservedProcesses.ContainsKey($processKey) -and
                        $ObservedProcesses[$processKey].created_utc -ne $evidence.created_utc) {
                        [void] $Limitations.Add("Observed child PID reused; replacement excluded: PID $childId")
                        continue
                    }
                    if (-not $ObservedProcesses.ContainsKey($processKey)) { $ObservedProcesses[$processKey] = $evidence }
                    Observe-SupplierModules $childHandle $ObservedPaths $Limitations $evidence
                    $pending.Enqueue($evidence)
                } catch {
                    [void] $Limitations.Add((Protect-SupplierObservation "Owned child observation unavailable: PID $childId; $($_.Exception.Message)"))
                } finally {
                    if ($null -ne $childHandle) { $childHandle.Dispose() }
                }
            }
        } catch {
            [void] $Limitations.Add((Protect-SupplierObservation "Owned lineage enumeration unavailable: PID $parentId; $($_.Exception.Message)"))
        } finally {
            if ($null -ne $parentHandle -and $parentId -ne $RootProcess.Id) { $parentHandle.Dispose() }
        }
    }
}

function Invoke-SupplierReceipt {
    param(
        [Parameter(Mandatory=$true)][string] $Name,
        [Parameter(Mandatory=$true)][string] $Scenario,
        [Parameter(Mandatory=$true)][string] $Profile,
        [Parameter(Mandatory=$true)][string] $Exe,
        [ValidatePattern('^(?:[a-z0-9]+(?:-[a-z0-9]+)*)?$')][string] $PhasePrefix = '',
        [ValidateRange(1,1800)][int] $TimeoutSeconds = 600,
        [switch] $Restart
    )
    if ($PhasePrefix) {
        if (-not $Name.StartsWith('native-')) { throw 'Phase-qualified receipt names must start with native-' }
        $Name = $Name -replace '^native-', "native-$PhasePrefix-"
        $Profile = "$PhasePrefix-$Profile"
    }
    $out = Join-Path $PSScriptRoot $Name
    $profilePath = Join-Path $PSScriptRoot "profile/$Profile"
    if (Test-Path -LiteralPath $out) { throw "Receipt already exists: $out" }
    if ($Restart) {
        if (-not (Test-Path -LiteralPath $profilePath -PathType Container)) {
            throw "Restart profile does not exist: $profilePath"
        }
    } elseif (Test-Path -LiteralPath $profilePath) {
        throw "Fresh profile already exists: $profilePath"
    }
    $exePath = (Resolve-Path -LiteralPath $Exe).Path
    $exeSha256 = (Get-FileHash -LiteralPath $exePath -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    $exeHashedUtc = [DateTime]::UtcNow.ToString('o')
    $scenarioPath = (Resolve-Path -LiteralPath (Join-Path $receiptRepository $Scenario)).Path
    New-Item -ItemType Directory -Path $out | Out-Null
    $env:TURNSTONE_SCENARIO = $scenarioPath
    $env:TURNSTONE_CAPTURE_DIR = $out
    $env:TURNSTONE_ROOT = $profilePath
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $exePath
    $start.WorkingDirectory = $receiptRepository
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = New-Object System.Text.UTF8Encoding($false)
    $start.StandardErrorEncoding = New-Object System.Text.UTF8Encoding($false)
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $start
    $timedOut = $false
    $nativeExit = $null
    $ownedPid = $null
    $started = [DateTime]::UtcNow
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    $record = [ordered]@{
        executable=$exePath; executable_sha256=$exeSha256; executable_hashed_utc=$exeHashedUtc;
        scenario=$scenarioPath; profile=$profilePath; phase_prefix=$PhasePrefix;
        owned_pid=$null; started_utc=$started.ToString('o'); finished_utc=$null;
        timeout_seconds=$TimeoutSeconds; timed_out=$false; native_exit=$null;
        module_evidence='loaded-modules.json'; scenario_result=$null;
        runner_error=$null; cleanup_attempted=$false; cleanup_error=$null;
        environment=[ordered]@{
            RUST_LOG=$env:RUST_LOG; SCRY_FIXTURE_BASE=$env:SCRY_FIXTURE_BASE;
            SERVO_FIXTURE_BASE=$env:SERVO_FIXTURE_BASE; TURNSTONE_CEF_PATH=$env:TURNSTONE_CEF_PATH;
            TURNSTONE_SERVO_PROFILE=$env:TURNSTONE_SERVO_PROFILE;
            TURNSTONE_SERVO_PROFILE_DIR=$env:TURNSTONE_SERVO_PROFILE_DIR
        }
    }
    # Persist executable identity before launch, including when launch fails.
    [System.IO.File]::WriteAllText((Join-Path $out 'process-result.json'),
        ($record | ConvertTo-Json -Depth 4), $utf8)
    try {
        if (-not $process.Start()) { throw "Native process could not start: $Name" }
        $ownedPid = $process.Id
        $record.owned_pid = $ownedPid
        $observedPaths = @{}
        $observedProcesses = @{}
        $observerLimitations = New-Object 'System.Collections.Generic.HashSet[string]'
        $rootStartedUtc = $null
        try {
            $rootStartedUtc = $process.StartTime.ToUniversalTime()
        } catch {
            [void] $observerLimitations.Add((Protect-SupplierObservation "Owned root creation time unavailable; child observation disabled: $($_.Exception.Message)"))
        }
        if ($null -ne $rootStartedUtc) { $record.started_utc = $rootStartedUtc.ToString('o') }
        [System.IO.File]::WriteAllText((Join-Path $out 'process-result.json'),
            ($record | ConvertTo-Json -Depth 4), $utf8)
        Write-Host "Running $Name (owned PID $ownedPid)"
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $rootEvidence = [ordered]@{
            pid=$ownedPid; parent_pid=$PID;
            created_utc=$(if ($null -ne $rootStartedUtc) { $rootStartedUtc.ToString('o') } else { $null });
            command_line_flags=@(); lineage='process handle started by this runner';
            first_observed_utc=[DateTime]::UtcNow.ToString('o')
        }
        $observedProcesses["$ownedPid"] = $rootEvidence
        $snapshotCount = 0
        $deadlineClock = [System.Diagnostics.Stopwatch]::StartNew()
        $exited = $false
        while ($deadlineClock.ElapsedMilliseconds -lt ($TimeoutSeconds * 1000)) {
            Observe-SupplierModules $process $observedPaths $observerLimitations $rootEvidence
            if ($null -ne $rootStartedUtc) {
                Observe-SupplierChildren $process $rootStartedUtc $observedPaths $observedProcesses $observerLimitations
            }
            $snapshotCount++
            $remaining = ($TimeoutSeconds * 1000) - $deadlineClock.ElapsedMilliseconds
            if ($remaining -le 0) { break }
            $waitMilliseconds = [int] [Math]::Min(250, $remaining)
            if ($process.WaitForExit($waitMilliseconds)) { $exited = $true; break }
        }
        $deadlineClock.Stop()
        if (-not $exited -and $process.HasExited) { $exited = $true }
        if (-not $exited) {
            $timedOut = $true
            # This handle identifies only the application this invocation created.
            # Never terminate by executable name or kill an unrelated process tree.
            if (-not $process.HasExited) { $process.Kill() }
            if (-not $process.WaitForExit(5000)) {
                throw "Owned native PID $ownedPid did not exit after timeout termination"
            }
        }
        $nativeExit = $process.ExitCode
        # Preserve termination state even if module serialization or drain fails.
        $record.finished_utc = [DateTime]::UtcNow.ToString('o')
        $record.timed_out = $timedOut
        $record.native_exit = $nativeExit
        [System.IO.File]::WriteAllText((Join-Path $out 'process-result.json'),
            ($record | ConvertTo-Json -Depth 4), $utf8)
        $utf8 = New-Object System.Text.UTF8Encoding($false)
        $moduleEvidence = [ordered]@{
            owned_pid=$ownedPid; sampling_wait_milliseconds=250; snapshots_attempted=$snapshotCount;
            observed_modules=@($observedPaths.Values | Sort-Object path);
            observed_processes=@($observedProcesses.Values | Sort-Object pid);
            observer_limitations=@($observerLimitations | Sort-Object);
            scope='Read-only owned root and live descendants carrying a --type role marker, with verified parent/creation identity; no child termination';
            inference_limit='Sampling can miss short-lived/exited/access-restricted children or parents without a role marker; role alone does not prove CEF; missing module is not proven absent; no staging-path inference';
            identity_limit='CIM and Process.StartTime creation timestamps agree within less than 1ms; lineage traversal limited to 64 processes per snapshot';
            command_line_policy='Only the bounded --type role marker is retained; full command lines and other flags are excluded'
        }
        [System.IO.File]::WriteAllText((Join-Path $out 'loaded-modules.json'),
            ($moduleEvidence | ConvertTo-Json -Depth 6), $utf8)
        if (-not $stdoutTask.Wait(5000) -or -not $stderrTask.Wait(5000)) {
            throw "Owned PID $ownedPid exited but redirected streams did not close within the bounded drain"
        }
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        [System.IO.File]::WriteAllText((Join-Path $out 'run.stdout.log'), $stdout, $utf8)
        [System.IO.File]::WriteAllText((Join-Path $out 'run.stderr.log'), $stderr, $utf8)
        # Separate streams preserve their own order; this combined file is for reading.
        [System.IO.File]::WriteAllText((Join-Path $out 'run.log'),
            "=== stdout ===`n$stdout`n=== stderr ===`n$stderr", $utf8)
        if (-not $timedOut -and $nativeExit -eq 0) {
            $done = Join-Path $out 'scenario.done'
            if (Test-Path -LiteralPath $done) {
                $record.scenario_result = Get-Content -LiteralPath $done -Encoding UTF8 -First 1
            }
        }
        [System.IO.File]::WriteAllText((Join-Path $out 'process-result.json'),
            ($record | ConvertTo-Json -Depth 4), $utf8)
        if ($timedOut) { throw "Native process exceeded ${TimeoutSeconds}s deadline: $Name" }
        if ($nativeExit -ne 0) { throw "Native process failed ($nativeExit): $Name" }
        if ($record.scenario_result -ne 'RESULT ok') { throw "Scenario failed or missing RESULT ok: $Name" }
        Write-Host "${Name}: $($record.scenario_result); native exit $nativeExit"
    } catch {
        $runnerFailure = $_
        $record.runner_error = Protect-SupplierObservation $runnerFailure.Exception.Message
        # A runner failure cannot remain accepted even if scenario.done was ok.
        $record.observed_scenario_result_before_runner_error = $record.scenario_result
        $record.scenario_result = $null
        $record.timed_out = $timedOut
        if ($null -ne $ownedPid) {
            # Cleanup owns only the handle started above, never children or names.
            try {
                if (-not $process.HasExited) {
                    $record.cleanup_attempted = $true
                    $process.Kill()
                    if (-not $process.WaitForExit(5000)) {
                        $record.cleanup_error = "Owned native PID $ownedPid did not exit after failure cleanup"
                    }
                }
                if ($process.HasExited) {
                    $record.native_exit = $process.ExitCode
                    if ($null -eq $record.finished_utc) { $record.finished_utc = [DateTime]::UtcNow.ToString('o') }
                }
            } catch {
                $record.cleanup_error = Protect-SupplierObservation $_.Exception.Message
            }
        }
        try {
            [System.IO.File]::WriteAllText((Join-Path $out 'process-result.json'),
                ($record | ConvertTo-Json -Depth 4), $utf8)
        } catch {
            Write-Warning -WarningAction Continue (Protect-SupplierObservation "Failed to persist native failure state: $($_.Exception.Message)")
        }
        throw $runnerFailure
    } finally {
        $process.Dispose()
    }
}
