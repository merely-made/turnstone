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
function Convert-OwnedRuntimeIdParts {
    param([Parameter(Mandatory=$true)][int[]] $RuntimeId)
    # Windows 0.32.1 appends four words of consumer 0.35's composite u128:
    # (local_id << 64) | tree_index. Reinterpret signed i32 words as raw bytes;
    # tree_index is the consumer's internal index, never a supplier UUID.
    if ($RuntimeId.Length -lt 5 -or $RuntimeId[-5] -ne 4) { throw 'Unexpected AccessKit Windows runtime identity encoding' }
    [byte[]]$localBytes=[BitConverter]::GetBytes($RuntimeId[-3])+[BitConverter]::GetBytes($RuntimeId[-4])
    [byte[]]$indexBytes=[BitConverter]::GetBytes($RuntimeId[-1])+[BitConverter]::GetBytes($RuntimeId[-2])
    return [ordered]@{local_node_id=[BitConverter]::ToUInt64($localBytes,0); consumer_tree_index=[BitConverter]::ToUInt64($indexBytes,0)}
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
    scope='LIMITED Windows UIA exposure of two original supplier HEADINGS and their owned-window ancestry; general body text, assistive actions, human AT and other platforms remain unqualified'
    observer_started_utc=[DateTime]::UtcNow.ToString('o')
    checkpoint_timeout_seconds=$TimeoutSeconds; traversal_timeout_seconds=$TimeoutSeconds
    checkpoint_wait_seconds=$null; traversal_seconds=$null
    hierarchy='Matched marker ancestry queried with actual UIA RawViewWalker parent RuntimeIds through the verified owned window root; at most 64 ancestors'
    text_limit='Known public fixture only; exact Heading Name properties, protected matched nodes refused, no aggregate TextPattern text read; original TOP body Name queries measured separately'
    probe='Prime provider once, retry exact current Heading Name PropertyConditions every 100ms, collect details only after both match'
    marker_kind='HEADINGS'; expected_headings=@('Servo A semantics','Servo B semantics')
    general_body_text_gate='OPEN: TOP TextRun/GenericContainer nodes are filtered by Windows consumer; RootWebArea lacks supplier text-range support'
    identity_encoding='accesskit_windows 0.32.1 node.rs40-50; accesskit_consumer 0.35.0 node.rs45-50: (local_id<<64)|tree_index'
    identity_limit='UIA exposes local node and internal consumer tree index, not supplier UUID. UUID association uses the unique original Heading label/local-id witness in the exact composed checkpoint'
    checkpoint_sha256=$null; body_name_probe_count=0; body_name_matches=@()
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
    $supplier=Get-Content -LiteralPath $checkpoint -Raw | ConvertFrom-Json
    if (-not $supplier.public_fixture_text) { throw 'Heading witness checkpoint did not admit public fixture text' }
    $record.checkpoint_sha256=(Get-FileHash -LiteralPath $checkpoint -Algorithm SHA256).Hash.ToLowerInvariant()
    # Loading the first two pages consumes only the checkpoint deadline. UIA
    # activation and its replay get a fresh independently bounded deadline.
    $traversalClock = [Diagnostics.Stopwatch]::StartNew()
    $process = [Diagnostics.Process]::GetProcessById([int]$launch.owned_pid)
    $expected = Convert-OwnedLaunchTimestamp $launch.started_utc
    if ($process.HasExited -or $process.StartTime.ToUniversalTime() -ne $expected) { throw 'Owned process identity no longer matches the launch receipt' }
    $record.owned_pid=$process.Id
    $record.owned_created_utc=$process.StartTime.ToUniversalTime().ToString('o')
    $primed=$false
    $markers=$record.expected_headings
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
        foreach ($bodyMarker in @('TOP A: red','TOP B: red')) {
            if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA body Name probe exceeded its deadline' }
            $bodyCondition=[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty,$bodyMarker)
            $bodyElement=$root.FindFirst([System.Windows.Automation.TreeScope]::Subtree,$bodyCondition)
            $record.body_name_probe_count++
            if ($null -ne $bodyElement) { $record.body_name_matches += $bodyMarker }
        }
        $rootRuntimeId=@($root.GetRuntimeId())
        $rootKey=$rootRuntimeId -join ','
        $record.root_runtime_id=$rootRuntimeId
        $rows=@()
        foreach ($element in $markerElements) {
            if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA traversal exceeded its deadline' }
            $current=$element.Current
            if ($current.IsPassword -or -not ($markers -contains $current.Name)) { throw 'UIA marker changed or is protected' }
            $runtimeId=@($element.GetRuntimeId())
            $parts=Convert-OwnedRuntimeIdParts $runtimeId
            if ($parts.consumer_tree_index -eq 0) { throw 'Heading was exposed in the host tree instead of a foreign subtree' }
            $witnesses=@(foreach ($surface in $supplier.surfaces) {
                foreach ($tree in $surface.trees) {
                    foreach ($node in $tree.nodes) {
                        if ($node.role -eq 'Heading' -and $node.label -eq $current.Name -and [uint64]$node.node_id -eq $parts.local_node_id) {
                            [ordered]@{surface=$surface.surface; root_tree_id=$surface.root_tree_id; tree_id=$tree.tree_id; local_node_id=$node.node_id; label=$node.label}
                        }
                    }
                }
            })
            if ($witnesses.Count -ne 1) { throw 'UIA Heading does not uniquely match the original composed supplier label/local identity' }
            $seen=@{}
            $ancestry=@()
            $documents=@()
            $ancestor=$element
            $associated=$false
            for ($depth=0; $depth -lt 64; $depth++) {
                if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA ancestry exceeded its deadline' }
                $ancestorId=@($ancestor.GetRuntimeId())
                $ancestorKey=$ancestorId -join ','
                if ($seen.ContainsKey($ancestorKey)) { throw 'UIA ancestry contains a cycle' }
                $seen[$ancestorKey]=$true
                $ancestry += ,$ancestorId
                if ($ancestor.Current.ControlType -eq [System.Windows.Automation.ControlType]::Document) {
                    $documentPatterns=@($ancestor.GetSupportedPatterns())
                    $documents += [ordered]@{runtime_id=$ancestorId; patterns=@($documentPatterns | ForEach-Object {$_.ProgrammaticName}); text_pattern_available=($documentPatterns.Id -contains [System.Windows.Automation.TextPattern]::Pattern.Id)}
                }
                if ($ancestorKey -eq $rootKey) { $associated=$true; break }
                $ancestor=$walker.GetParent($ancestor)
                if ($null -eq $ancestor) { break }
            }
            if (-not $associated) { throw 'UIA marker ancestry does not reach the owned window root within 64 ancestors' }
            $patterns=@($element.GetSupportedPatterns())
            $rows += [ordered]@{
                runtime_id=$runtimeId; control_type=$current.ControlType.ProgrammaticName
                local_node_id=$parts.local_node_id; consumer_tree_index=$parts.consumer_tree_index; supplier_heading_witness=$witnesses[0]
                parent_runtime_id=$ancestry[1]; ancestry_runtime_ids=$ancestry; associated_with_owned_root=$associated
                document_ancestor_patterns=$documents
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
        if ($rows[0].consumer_tree_index -eq $rows[1].consumer_tree_index) { throw 'Independent supplier Headings share one consumer tree index' }
        $process.Refresh()
        if ($process.HasExited -or $process.StartTime.ToUniversalTime() -ne $expected) { throw 'Owned process ended before UIA acknowledgment' }
        if ($traversalClock.Elapsed.TotalSeconds -ge $TimeoutSeconds) { throw 'UIA acknowledgment exceeded its deadline' }
        $ack=[ordered]@{passed=$true; scope=$record.scope; marker_kind='HEADINGS'; observed_utc=[DateTime]::UtcNow.ToString('o'); owned_pid=$process.Id; owned_created_utc=$record.owned_created_utc; hwnd=$record.hwnd; headings=$record.observed_markers; checkpoint_sha256=$record.checkpoint_sha256; root_runtime_id=$rootRuntimeId; nodes=$rows; body_name_probe_count=$record.body_name_probe_count; body_name_matches=$record.body_name_matches; general_body_text_gate=$record.general_body_text_gate; identity_limit=$record.identity_limit}
        $ackBytes=[Text.Encoding]::UTF8.GetBytes(($ack | ConvertTo-Json -Depth 8))
        $ackFile=[IO.File]::Open($readyPath,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try { $ackFile.Write($ackBytes,0,$ackBytes.Length) } finally { $ackFile.Dispose() }
        $record.passed=$true
        break
    }
    if (-not $record.passed) { throw 'UIA did not expose both original supplier Headings and their ancestry within the deadline' }
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
