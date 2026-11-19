# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0

param(
    [Parameter(Mandatory)][int]$OwnerPid,
    [Parameter(Mandatory)][long]$Hwnd,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$TargetLabel,
    [Parameter(Mandatory)][string]$SiblingLabel
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

function Save-Receipt([string]$Name, [object]$Value) {
    $path = Join-Path $OutputDirectory $Name
    $temporary = "$path.partial"
    if ((Test-Path -LiteralPath $path) -or (Test-Path -LiteralPath $temporary)) {
        throw "Refusing to overwrite receipt $path"
    }
    [IO.File]::WriteAllText($temporary, (ConvertTo-Json -InputObject $Value -Depth 8))
    [IO.File]::Move($temporary, $path)
}

function Owned-Window {
    $root = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
    if (!$root -or $root.Current.ProcessId -ne $OwnerPid -or $root.Current.NativeWindowHandle -ne $Hwnd) {
        throw "HWND $Hwnd is not the supplied process's native window"
    }
    return $root
}

function Find-Named([Windows.Automation.AutomationElement]$Window, [string]$Name) {
    $condition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    return ,($Window.FindAll([Windows.Automation.TreeScope]::Descendants, $condition))
}

function Ancestry([Windows.Automation.AutomationElement]$Node) {
    $path = @()
    $walker = [Windows.Automation.TreeWalker]::RawViewWalker
    for ($depth = 0; $Node -and $depth -lt 40; $depth++) {
        $id = $Node.Current.ProcessId
        if ($id -ne $OwnerPid) { throw "The UIA target's ancestry left the owned process" }
        $handle = $Node.Current.NativeWindowHandle
        $path += [ordered]@{
            runtime_id = @($Node.GetRuntimeId())
            control_type = $Node.Current.ControlType.ProgrammaticName
            hwnd = $handle
        }
        if ($handle -eq $Hwnd) { return ,$path }
        $Node = $walker.GetParent($Node)
    }
    throw "The UIA target has no ancestry to the supplied HWND"
}

$owner = Get-Process -Id $OwnerPid
$start = $owner.StartTime.ToUniversalTime().ToString('o')
$until = [DateTime]::UtcNow.AddSeconds(30)
$target = $null
$sibling = $null
while ([DateTime]::UtcNow -lt $until) {
    $root = Owned-Window
    $targets = Find-Named $root $TargetLabel
    $siblings = Find-Named $root $SiblingLabel
    if ($targets.Count -gt 1 -or $siblings.Count -gt 1) {
        throw 'Expected unique public fixture labels inside the owned HWND'
    }
    if ($targets.Count -eq 1 -and $siblings.Count -eq 1) {
        $target = $targets.Item(0)
        $sibling = $siblings.Item(0)
        break
    }
    Start-Sleep -Milliseconds 125
}
if (!$target -or !$sibling) { throw 'Nested supplier controls did not appear on the owned HWND' }
if ($target.Current.ControlType -ne [Windows.Automation.ControlType]::Button -or
    $sibling.Current.ControlType -ne [Windows.Automation.ControlType]::Button) {
    throw 'The supplier controls were not exposed as native UIA buttons'
}
$pattern = $null
if (!$target.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    throw 'Target did not advertise Windows UIA InvokePattern'
}
$targetAncestry = Ancestry $target
$siblingAncestry = Ancestry $sibling
if (($target.GetRuntimeId() -join '.') -eq ($sibling.GetRuntimeId() -join '.')) {
    throw 'The two trees exposed the same native RuntimeId'
}
([Windows.Automation.InvokePattern]$pattern).Invoke()
Save-Receipt 'uia-invoked.json' ([ordered]@{
    owner_pid = $OwnerPid
    process_started_utc = $start
    hwnd = $Hwnd
    native_pattern = 'InvokePattern'
    target_fixture_label = $TargetLabel
    sibling_fixture_label = $SiblingLabel
    target_ancestry = $targetAncestry
    sibling_ancestry = $siblingAncestry
})

$until = [DateTime]::UtcNow.AddSeconds(30)
while ([DateTime]::UtcNow -lt $until) {
    if (Test-Path -LiteralPath (Join-Path $OutputDirectory 'supplier-retired.ready')) {
        $root = Owned-Window
        if ((Find-Named $root $TargetLabel).Count -eq 0 -and (Find-Named $root $SiblingLabel).Count -eq 0) {
            Save-Receipt 'uia-withdrawn.json' ([ordered]@{
                owner_pid = $OwnerPid
                hwnd = $Hwnd
                fixture_targets_remaining = 0
            })
            exit 0
        }
    }
    Start-Sleep -Milliseconds 125
}
throw 'Nested fixture controls stayed in the owned native tree after retirement'
