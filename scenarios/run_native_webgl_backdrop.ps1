# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$ArtifactDir,
    [ValidateSet('bole', 'vano')][string]$Engine = 'bole'
)

$ErrorActionPreference = 'Stop'
$Engine = $Engine.ToLowerInvariant()
$exe = (Resolve-Path -LiteralPath $Executable).Path
$fixture = Join-Path $PSScriptRoot 'fixtures/native_webgl_backdrop.html'
$artifact = [IO.Path]::GetFullPath($ArtifactDir)
if (Test-Path -LiteralPath $artifact) { throw 'Use a fresh artifact directory.' }
New-Item -ItemType Directory -Path $artifact | Out-Null

$serverCode = @'
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sys

page = Path(sys.argv[1]).read_bytes()

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/backdrop.html":
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(page)))
        self.end_headers()
        self.wfile.write(page)

with ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
    Path(sys.argv[2]).write_text(f"http://127.0.0.1:{server.server_port}/backdrop.html")
    server.serve_forever()
'@
$addressFile = Join-Path $artifact 'address.txt'
$start = [Diagnostics.ProcessStartInfo]::new((Get-Command python).Source)
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
foreach ($argument in @('-c', $serverCode, $fixture, $addressFile)) {
    $start.ArgumentList.Add($argument)
}
$server = [Diagnostics.Process]::Start($start)
$vars = @{
    TURNSTONE_ROOT = Join-Path $artifact 'profile'
    TURNSTONE_CAPTURE_DIR = $artifact
    TURNSTONE_SCENARIO = Join-Path $artifact 'backdrop.scn'
    TURNSTONE_GENET_ENGINE = $Engine
    TURNSTONE_OFFLINE = '0'
}
$previous = @{}
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (!(Test-Path -LiteralPath $addressFile) -or !(Get-Item -LiteralPath $addressFile).Length) {
        if ($server.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Local fixture server failed to start.' }
        Start-Sleep -Milliseconds 50
    }
    $address = Get-Content -LiteralPath $addressFile -Raw
    @"
settle 30
key escape
open $address
settle 45
assert visible
act Toggle live content
settle 60
assert content-live
assert content-ready
assert a11y Backdrop blur receipt complete
record-idle backdrop
capture backdrop
"@ | Set-Content -LiteralPath $vars.TURNSTONE_SCENARIO -Encoding utf8
    foreach ($item in $vars.GetEnumerator()) {
        $previous[$item.Key] = [Environment]::GetEnvironmentVariable($item.Key)
        [Environment]::SetEnvironmentVariable($item.Key, $item.Value)
    }
    Push-Location (Split-Path $PSScriptRoot -Parent)
    try {
        & $exe 2>&1 | Tee-Object -FilePath (Join-Path $artifact 'turnstone.log')
        if ($LASTEXITCODE -ne 0) { throw "Turnstone exited with $LASTEXITCODE." }
    } finally { Pop-Location }
    $done = Get-Content -LiteralPath (Join-Path $artifact 'scenario.done') -TotalCount 1
    if ($done -ne 'RESULT ok') { throw "Scenario failed. See $artifact." }
    $diagnostic = Get-Content -LiteralPath (Join-Path $artifact 'backdrop.cue-frames.json') -Raw | ConvertFrom-Json
    $engineId = if ($Engine -eq 'vano') { 'genet.scripted.nova' } else { 'genet.scripted' }
    $entries = @($diagnostic.entries | Where-Object { $_.direction -eq 'host-device' -and $_.webgl_images -eq 1 })
    if ($entries.Count -ne 1) { throw 'Expected one staged WebGL canvas on a host-device content surface.' }
    if ($entries[0].engine_id -ne $engineId) { throw "Expected $engineId, got $($entries[0].engine_id)." }
    $bounds, $scale = $entries[0].viewport
    if ($null -eq $scale -or $scale -le 0) { throw 'Missing document viewport and scale.' }

    Add-Type -AssemblyName System.Drawing
    $bitmap = [Drawing.Bitmap]::FromFile((Join-Path $artifact 'backdrop.png'))
    try {
        $samples = @(
            @{ name='outside-pane'; point=@(43,52); rgb=@(255,0,255) },
            @{ name='mirrored-pane-edge'; point=@(44.5,52); rgb=@(0,255,0) },
            @{ name='refreshed-WebGL-canvas'; point=@(62,52); rgb=@(0,255,0) },
            @{ name='canvas-edge-blurred'; point=@(90,52); mixed=$true },
            @{ name='sharp-before-source-edge'; point=@(89,91); rgb=@(0,255,0) },
            @{ name='sharp-after-source-edge'; point=@(91,91); rgb=@(255,255,255) },
            @{ name='sharp-pane-child'; point=@(77,73); rgb=@(0,0,255) },
            @{ name='sharp-pane-child-overflow'; point=@(77,84); rgb=@(0,0,255) },
            @{ name='later-auto-z-index-foreground'; point=@(90,72); rgb=@(255,0,0) }
        )
        $rows = foreach ($sample in $samples) {
            $x = [int][Math]::Floor($bounds[0] + $sample.point[0] * $scale)
            $y = [int][Math]::Floor($bounds[1] + $sample.point[1] * $scale)
            $p = $bitmap.GetPixel($x, $y)
            $rgb = @($p.R,$p.G,$p.B)
            if ($sample.mixed) {
                $expected = '48..210,250..255,48..210 with matching red and blue'
                $matches = $p.R -ge 48 -and $p.R -le 210 -and $p.G -ge 250 -and [Math]::Abs([int]$p.R-[int]$p.B) -le 2
            } else {
                $expected = $sample.rgb -join ','
                $matches = (0..2 | Where-Object { [Math]::Abs([int]$rgb[$_]-[int]$sample.rgb[$_]) -gt 2 }).Count -eq 0
            }
            if (!$matches -or $p.A -ne 255) {
                throw "$($sample.name) ($x,$y) was $($rgb -join ','),$($p.A), expected $expected."
            }
            [pscustomobject]@{ name=$sample.name; x=$x; y=$y; expected=$expected; actual="$($rgb -join ','),$($p.A)" }
        }
        [pscustomobject]@{ engine=$Engine; engine_id=$engineId; scale=$scale; viewport=$bounds; samples=@($rows) } |
            ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $artifact 'backdrop-pixels.json')
        Write-Host "$Engine passed $($rows.Count) native backdrop pixel checks."
    } finally { $bitmap.Dispose() }
} finally {
    if (!$server.HasExited) { $server.Kill(); $server.WaitForExit() }
    $server.Dispose()
    foreach ($item in $previous.GetEnumerator()) {
        [Environment]::SetEnvironmentVariable($item.Key, $item.Value)
    }
}
