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
$record = [ordered]@{
    scope='Windows UI Automation traversal of the owned public-fixture process; not a human screen-reader walk or other-platform qualification'
    observer_started_utc=[DateTime]::UtcNow.ToString('o')
    checkpoint_timeout_seconds=$TimeoutSeconds; traversal_timeout_seconds=$TimeoutSeconds
    checkpoint_wait_seconds=$null; traversal_seconds=$null
    hierarchy='Actual parent RuntimeId queried with UIA RawViewWalker; the window parent may be outside this owned subtree'
    text_limit='Known public fixture only; IsPassword redacts the current element, but ancestor TextPattern text is not a protected-descendant filter'
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
    while ($traversalClock.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        $process.Refresh()
        if ($process.HasExited -or $process.StartTime.ToUniversalTime() -ne $expected) { throw 'Owned process ended or changed identity before the UIA traversal completed' }
        $hwnd=$process.MainWindowHandle
        if ($hwnd -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100; continue }
        $record.hwnd=$hwnd.ToInt64()
        $root=[System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
        if ($root.Current.ProcessId -ne $process.Id) { throw 'UIA root does not belong to the owned process' }
        $elements=$root.FindAll([System.Windows.Automation.TreeScope]::Subtree,[System.Windows.Automation.Condition]::TrueCondition)
        $walker=[System.Windows.Automation.TreeWalker]::RawViewWalker
        $rows=@()
        foreach ($element in $elements) {
            if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA traversal exceeded its deadline' }
            $current=$element.Current
            $parent=$walker.GetParent($element)
            $parentRuntimeId=if ($null -eq $parent) { $null } else { @($parent.GetRuntimeId()) }
            $name=if ($current.IsPassword) { '[protected]' } else { $current.Name }
            $patterns=@($element.GetSupportedPatterns())
            $text=$null
            if (-not $current.IsPassword -and ($patterns.Id -contains [System.Windows.Automation.TextPattern]::Pattern.Id)) {
                $textPattern=[System.Windows.Automation.TextPattern]$element.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern)
                $text=$textPattern.DocumentRange.GetText(4096)
            }
            $rows += [ordered]@{
                runtime_id=@($element.GetRuntimeId()); control_type=$current.ControlType.ProgrammaticName
                parent_runtime_id=$parentRuntimeId
                name=$name; protected=$current.IsPassword; enabled=$current.IsEnabled
                offscreen=$current.IsOffscreen; focused=$current.HasKeyboardFocus
                bounds=@($current.BoundingRectangle.X,$current.BoundingRectangle.Y,$current.BoundingRectangle.Width,$current.BoundingRectangle.Height)
                patterns=@($patterns | ForEach-Object { $_.ProgrammaticName }); text=$text
            }
        }
        $record.nodes=$rows
        $readText=@($rows | ForEach-Object { $_.name; $_.text }) -join "`n"
        $record.observed_markers=@('TOP A: red','TOP B: red' | Where-Object { $readText.Contains($_) })
        if ($record.observed_markers.Count -eq 2) { $record.passed=$true; break }
        # The first UIA query activates AccessKit. The UI thread then replays
        # descendants; retry the actual provider rather than its cached DTO.
        Start-Sleep -Milliseconds 100
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
