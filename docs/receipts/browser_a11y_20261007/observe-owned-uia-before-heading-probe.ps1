# Copyright 2026 Mark Alan Boykin. SPDX-License-Identifier: MPL-2.0
# Read only the fresh public-fixture process started by the native runner.
param(
    [Parameter(Mandatory=$true)][string] $ReceiptDirectory,
    [ValidateRange(1,180)][int] $TimeoutSeconds = 90
)
$ErrorActionPreference = 'Stop'
function Convert-OwnedLaunchTimestamp {
    param([Parameter(Mandatory=$true)][object] $Value)
    # PowerShell 7 may deserialize JSON ISO timestamps directly as DateTime.
    # Re-parsing that object first formats it with the current culture and can
    # discard fractional seconds, invalidating an exact process identity check.
    if ($Value -is [DateTime]) { return $Value.ToUniversalTime() }
    if ($Value -is [DateTimeOffset]) { return $Value.UtcDateTime }
    if ($Value -isnot [string]) { throw 'Owned launch timestamp has an unexpected type' }
    return [DateTime]::ParseExact(
        $Value, 'o', [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::RoundtripKind
    ).ToUniversalTime()
}
if ($env:TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT -ne '1') {
    throw 'This UIA text receipt requires an explicitly admitted public fixture'
}
$out = [IO.Path]::GetFullPath($ReceiptDirectory)
$resultPath = Join-Path $out 'uia-tree.json'
if (Test-Path -LiteralPath $resultPath) { throw "Preserving existing UIA receipt: $resultPath" }
$readyPath = Join-Path $out 'uia-ready.json'
if (Test-Path -LiteralPath $readyPath) { throw "Preserving existing UIA acknowledgment: $readyPath" }
$record = [ordered]@{
    scope='Windows UI Automation traversal of the owned public-fixture process; not a human screen-reader walk or other-platform qualification'
    observer_started_utc=[DateTime]::UtcNow.ToString('o')
    checkpoint_timeout_seconds=$TimeoutSeconds; traversal_timeout_seconds=$TimeoutSeconds
    checkpoint_wait_seconds=$null; traversal_seconds=$null
    hierarchy='Matched marker ancestry queried with actual UIA RawViewWalker parent RuntimeIds through the verified owned window root; at most 64 ancestors'
    text_limit='Known public fixture only; exact marker Name properties only, protected matched nodes refused and no aggregate TextPattern text read'
    probe='Prime provider once, retry exact current Name PropertyConditions every 100ms, collect details only after both markers match'
    probe_attempts=0; root_runtime_id=$null; success_ack='uia-ready.json'
    owned_pid=$null; owned_created_utc=$null; hwnd=$null
    observed_markers=@(); nodes=@(); passed=$false; limitation=$null
}
$checkpointClock = [Diagnostics.Stopwatch]::StartNew()
$traversalClock = $null
$process = $null
try {
    Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
    $binding = Join-Path $out 'process-result.json'
    $checkpoint = Join-Path $out 'a11y-two-pages.foreign-a11y.json'
    while ($checkpointClock.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        if ((Test-Path -LiteralPath $binding) -and (Test-Path -LiteralPath $checkpoint)) {
            $launch = Get-Content -LiteralPath $binding -Raw | ConvertFrom-Json
            if ($null -ne $launch.owned_pid) { break }
        }
        Start-Sleep -Milliseconds 100
    }
    $checkpointClock.Stop()
    $record.checkpoint_wait_seconds=$checkpointClock.Elapsed.TotalSeconds
    if ($null -eq $launch -or $null -eq $launch.owned_pid) { throw 'Owned launch/checkpoint was not observed within the deadline' }
    # Loading the first two pages consumes only the checkpoint deadline. UIA
    # activation and its replay get a fresh independently bounded deadline.
    $traversalClock = [Diagnostics.Stopwatch]::StartNew()
    $process = [Diagnostics.Process]::GetProcessById([int]$launch.owned_pid)
    $expected = Convert-OwnedLaunchTimestamp $launch.started_utc
    if ($process.HasExited -or $process.StartTime.ToUniversalTime() -ne $expected) { throw 'Owned process identity no longer matches the launch receipt' }
    $record.owned_pid=$process.Id
    $record.owned_created_utc=$process.StartTime.ToUniversalTime().ToString('o')
    $primed=$false
    $markers=@('TOP A: red','TOP B: red')
    $walker=[System.Windows.Automation.TreeWalker]::RawViewWalker
    while ($traversalClock.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        $process.Refresh()
        if ($process.HasExited -or $process.StartTime.ToUniversalTime() -ne $expected) { throw 'Owned process ended or changed identity before the UIA traversal completed' }
        $hwnd=$process.MainWindowHandle
        if ($hwnd -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100; continue }
        $record.hwnd=$hwnd.ToInt64()
        $root=[System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
        if ($root.Current.ProcessId -ne $process.Id) { throw 'UIA root does not belong to the owned process' }
        if (-not $primed) {
            # This first child query activates the provider on its safe host
            # snapshot. Let the UI thread replay before probing real markers.
            $null=$root.FindFirst([System.Windows.Automation.TreeScope]::Children,[System.Windows.Automation.Condition]::TrueCondition)
            $primed=$true
            Start-Sleep -Milliseconds 100
            continue
        }
        $record.probe_attempts++
        $markerElements=@()
        foreach ($marker in $markers) {
            if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA marker probe exceeded its deadline' }
            $condition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$marker)
            $match=$root.FindFirst([System.Windows.Automation.TreeScope]::Subtree,$condition)
            if ($null -ne $match) { $markerElements += $match }
        }
        if ($markerElements.Count -ne 2) { Start-Sleep -Milliseconds 100; continue }
        $rootRuntimeId=@($root.GetRuntimeId())
        $rootKey=$rootRuntimeId -join ','
        $record.root_runtime_id=$rootRuntimeId
        $rows=@()
        foreach ($element in $markerElements) {
            if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA traversal exceeded its deadline' }
            $current=$element.Current
            if ($current.IsPassword -or -not ($markers -contains $current.Name)) { throw 'UIA marker changed or is protected' }
            $runtimeId=@($element.GetRuntimeId())
            $seen=@{}
            $ancestry=@()
            $ancestor=$element
            $associated=$false
            for ($depth=0; $depth -lt 64; $depth++) {
                if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA ancestry exceeded its deadline' }
                $ancestorId=@($ancestor.GetRuntimeId())
                $ancestorKey=$ancestorId -join ','
                if ($seen.ContainsKey($ancestorKey)) { throw 'UIA ancestry contains a cycle' }
                $seen[$ancestorKey]=$true
                $ancestry += ,$ancestorId
                if ($ancestorKey -eq $rootKey) { $associated=$true; break }
                $ancestor=$walker.GetParent($ancestor)
                if ($null -eq $ancestor) { break }
            }
            if (-not $associated) { throw 'UIA marker ancestry does not reach the owned window root within 64 ancestors' }
            $patterns=@($element.GetSupportedPatterns())
            $rows += [ordered]@{
                runtime_id=$runtimeId; control_type=$current.ControlType.ProgrammaticName
                parent_runtime_id=$ancestry[1]; ancestry_runtime_ids=$ancestry; associated_with_owned_root=$associated
                name=$current.Name; protected=$current.IsPassword; enabled=$current.IsEnabled
                offscreen=$current.IsOffscreen; focused=$current.HasKeyboardFocus
                bounds=@($current.BoundingRectangle.X,$current.BoundingRectangle.Y,$current.BoundingRectangle.Width,$current.BoundingRectangle.Height)
                patterns=@($patterns | ForEach-Object { $_.ProgrammaticName }); text=$null
            }
        }
        $record.nodes=$rows
        $record.observed_markers=@($markers | Where-Object { $rows.name -contains $_ })
        if ($record.observed_markers.Count -ne 2) { throw 'UIA exact marker verification failed' }
        if (($rows[0].runtime_id -join ',') -eq ($rows[1].runtime_id -join ',')) { throw 'Independent markers share one UIA runtime identity' }
        $process.Refresh()
        if ($process.HasExited -or $process.StartTime.ToUniversalTime() -ne $expected) { throw 'Owned process ended before UIA acknowledgment' }
        if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA acknowledgment exceeded its deadline' }
        $ack=[ordered]@{passed=$true; observed_utc=[DateTime]::UtcNow.ToString('o'); owned_pid=$process.Id; owned_created_utc=$record.owned_created_utc; hwnd=$record.hwnd; markers=$record.observed_markers; root_runtime_id=$rootRuntimeId; nodes=$rows}
        $ackBytes=[Text.Encoding]::UTF8.GetBytes(($ack | ConvertTo-Json -Depth 8))
        $ackFile=[IO.File]::Open($readyPath,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try { $ackFile.Write($ackBytes,0,$ackBytes.Length) } finally { $ackFile.Dispose() }
        $record.passed=$true
        break
    }
    if (-not $record.passed) { throw 'UIA did not expose both independent Servo page markers within the deadline' }
} catch {
    $record.limitation=$_.Exception.Message
} finally {
    $checkpointClock.Stop()
    $record.checkpoint_wait_seconds=$checkpointClock.Elapsed.TotalSeconds
    if ($null -ne $traversalClock) {
        $traversalClock.Stop()
        $record.traversal_seconds=$traversalClock.Elapsed.TotalSeconds
    }
    if ($null -ne $process) { $process.Dispose() }
    $record.observer_finished_utc=[DateTime]::UtcNow.ToString('o')
    if (Test-Path -LiteralPath $out -PathType Container) {
        [IO.File]::WriteAllText($resultPath,($record | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
    }
}
if (-not $record.passed) { throw $record.limitation }
