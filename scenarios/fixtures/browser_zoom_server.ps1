# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

param(
    [Parameter(Mandatory = $true)]
    [int] $Port,

    [Parameter(Mandatory = $true)]
    [string] $ReadyPath,

    [Parameter(Mandatory = $true)]
    [string] $ReceiptPath,

    [int] $TimeoutSeconds = 420
)

# E0.2 page-zoom acceptance fixture. Serves the pages whose LAYOUT distinguishes
# 100% from 125%, so a zoom receipt cannot pass by asserting a number the
# engine merely echoed back:
#
#   /reflow   a `@media (max-width: 550px)` boundary. The retained content rect
#             is 614 CSS px wide at 100% and 491 at 125%, so the boundary is
#             crossed exactly by the zoom step. Each branch carries its own
#             marker word and the other branch is `display: none`, so document
#             find counts 1/0 one way and 0/1 the other.
#   /hit      two 200px-tall block links. Presentation y=225 is CSS 225 (the
#             SECOND link) at 100% and CSS 180 (the FIRST) at 125%, so one
#             click point names which geometry the hit test used.
#   /hit2     the same page at a second address, so the 100% and 125% blocks
#             land on two different graph nodes (see the note beside it).
#   /reveal   a 1500px filler, then a 420px-tall link holding the find marker.
#             Reveal parks the match 24 CSS px below the viewport top, so the
#             link ends at presentation y=444 at 100% and y=555 at 125%.
#             Presentation y=500 therefore lands INSIDE the link only when the
#             reveal did its CSS/presentation conversion at 125%; before the
#             find it is filler at either scale.
#   /zoom     a plain page for the ladder + restart receipts.
#   /article  enough real prose for the reader lane's extractor to accept, so
#             the "an engine that reports no page zoom omits the rows" half can
#             be driven from the same fixture instead of a second server.
#
# Nothing here is timing-dependent and no page runs script: the fixture exists
# to make layout observable, not to drive the app.

$ErrorActionPreference = "Stop"

function Read-HttpRequest {
    param([System.IO.Stream] $Stream)

    # Chromium opens speculative connections and then says nothing on them.
    # This accept loop is serial, so a silent socket would block every later
    # request behind it — including Turnstone's own metadata fetch, which is
    # how a hosted receipt ends up clicking a page that has not arrived. A
    # short read timeout turns that into an abandoned connection instead.
    $Stream.ReadTimeout = 1500
    $bytes = [System.Collections.Generic.List[byte]]::new()
    [byte[]] $terminator = @(13, 10, 13, 10)
    $matched = 0
    while ($bytes.Count -le 32768 -and $matched -lt $terminator.Length) {
        $current = $Stream.ReadByte()
        if ($current -lt 0) {
            return $null
        }
        $bytes.Add([byte] $current)
        if ($current -eq $terminator[$matched]) {
            $matched += 1
        }
        else {
            $matched = if ($current -eq $terminator[0]) { 1 } else { 0 }
        }
    }
    if ($matched -ne $terminator.Length) {
        throw "HTTP header exceeded 32768 bytes"
    }

    $text = [System.Text.Encoding]::ASCII.GetString($bytes.ToArray())
    $lines = $text -split "`r`n"
    $requestParts = $lines[0] -split " ", 3
    if ($requestParts.Count -ne 3) {
        throw "bad HTTP request line: $($lines[0])"
    }
    [pscustomobject]@{
        Method = $requestParts[0]
        Target = $requestParts[1]
    }
}

function Write-HttpResponse {
    param(
        [System.IO.Stream] $Stream,
        [string] $Status,
        [string] $ContentType,
        [byte[]] $Body
    )

    $head = [System.Text.StringBuilder]::new()
    [void] $head.Append("HTTP/1.1 $Status`r`n")
    [void] $head.Append("Content-Type: $ContentType`r`n")
    [void] $head.Append("Content-Length: $($Body.Length)`r`n")
    [void] $head.Append("Cache-Control: no-store`r`n")
    [void] $head.Append("Connection: close`r`n")
    [void] $head.Append("`r`n")
    [byte[]] $headBytes = [System.Text.Encoding]::ASCII.GetBytes($head.ToString())
    $Stream.Write($headBytes, 0, $headBytes.Length)
    $Stream.Write($Body, 0, $Body.Length)
    $Stream.Flush()
}

$reset = "html,body{margin:0;padding:0;border:0}body{font:16px/1.2 system-ui,sans-serif;color:#172033;background:#f4efe3}p{margin:0;padding:0}"

$pages = @{}

$pages["/zoom"] = @"
<!doctype html>
<meta charset="utf-8">
<title>Turnstone zoom fixture</title>
<style>$reset</style>
<p id="a">ZOOMFIXTURE page for the per-node page-zoom ladder.</p>
<p id="b">documentation of the second paragraph.</p>
"@

$pages["/article"] = @"
<!doctype html>
<meta charset="utf-8">
<title>Turnstone reader-lane article</title>
<style>$reset article{max-width:40em;margin:0 auto;padding:24px}p{margin:0 0 16px}</style>
<article>
<h1>A page zoom is a document scale</h1>
<p>ARTICLEMARK. Page zoom changes the size of a document without changing the
size of the window that holds it. The engine shrinks the CSS viewport by the
zoom factor and lays the document out again against the smaller box, so a
media query written for a narrow screen answers as though the screen really
were narrower. Everything a page can measure about itself moves together.</p>
<p>That is what separates a user-agent page zoom from a CSS transform applied
to the root element. A transform scales pixels that were already laid out for
the old box, so the page keeps its wide-screen arrangement and simply becomes
larger. Nothing reflows, nothing responds, and the reader who wanted bigger
text ends up scrolling sideways to read each line.</p>
<p>It is also distinct from a chrome or interface zoom, which scales the
application's own furniture — the address field, the tabs, the panes — and
leaves the document alone. The two are frequently confused because both make
things look bigger, and both are commonly bound to the same modifier and
wheel gesture in different contexts.</p>
<p>The third scale in a graph workspace is the camera. A canvas camera zoom
moves the viewer through a scene of nodes; it has no opinion about the text
inside any one of them. A page can be at two hundred percent while its node
is a speck on the canvas, and neither number tells you anything about the
other.</p>
<p>Keeping the three separate is mostly a matter of naming and of never
letting one control write to another's state. The requested document scale
belongs to the node, persists with it, and is replayed into whatever engine
next presents that node. The engine decides only what it can actually honour:
the quantization, the bounds, and the effective level it settles on.</p>
<p>Not every engine offers the control at all. A reader lane that has already
reflowed an article into a single column of its own choosing has no page zoom
to give, and the honest answer is to say so and withhold the commands rather
than to accept a request it will silently drop.</p>
</article>
"@

# 550px sits between the 125% CSS width (491) and the 100% CSS width (614) of
# the retained content rect, so the media boundary is crossed by the zoom step
# itself rather than by a window resize.
$pages["/reflow"] = @"
<!doctype html>
<meta charset="utf-8">
<title>Turnstone reflow fixture</title>
<style>
$reset
#wide { display: block; }
#narrow { display: none; }
@media (max-width: 550px) {
  #wide { display: none; }
  #narrow { display: block; }
}
</style>
<p id="wide">LAYOUTWIDE</p>
<p id="narrow">LAYOUTNARROW</p>
"@

$pages["/hit"] = @"
<!doctype html>
<meta charset="utf-8">
<title>Turnstone hit-test fixture</title>
<style>
$reset
a { display: block; height: 200px; width: 100%; text-decoration: none; color: #172033; }
#top { background: #cfd9e8; }
#bottom { background: #d9e8cf; }
</style>
<a id="top" href="/hit-top">HITTOP</a>
<a id="bottom" href="/hit-bottom">HITBOTTOM</a>
"@

# A byte-identical second copy at its own address. The two hit-test blocks
# (100% and 125%) must land on DIFFERENT graph nodes: re-opening one address
# after its node has navigated away does not re-fetch, and the hosted lane
# then drives a surface whose document has not finished arriving.
$pages["/hit2"] = $pages["/hit"]

$pages["/hit-top"] = @"
<!doctype html>
<meta charset="utf-8">
<title>hit-top</title>
<style>$reset</style>
<p>ARRIVEDTOP</p>
"@

$pages["/hit-bottom"] = @"
<!doctype html>
<meta charset="utf-8">
<title>hit-bottom</title>
<style>$reset</style>
<p>ARRIVEDBOTTOM</p>
"@

$pages["/reveal"] = @"
<!doctype html>
<meta charset="utf-8">
<title>Turnstone find-reveal fixture</title>
<style>
$reset
#filler { height: 1500px; background: #ece5d6; }
#target { display: block; height: 420px; width: 100%; background: #cfd9e8; text-decoration: none; color: #172033; }
#tail { height: 1500px; background: #ece5d6; }
</style>
<div id="filler">filler above the match</div>
<a id="target" href="/landed-reveal">REVEALMARK is the find target</a>
<div id="tail">filler below the match</div>
"@

$pages["/landed-reveal"] = @"
<!doctype html>
<meta charset="utf-8">
<title>landed-reveal</title>
<style>$reset</style>
<p>ARRIVEDREVEAL</p>
"@

function Accept-BeforeDeadline {
    param(
        [System.Net.Sockets.TcpListener] $Listener,
        [datetime] $Deadline
    )

    while (-not $Listener.Pending()) {
        if ([datetime]::UtcNow -ge $Deadline) {
            return $null
        }
        Start-Sleep -Milliseconds 10
    }
    $Listener.AcceptTcpClient()
}

$readyDirectory = Split-Path -Parent $ReadyPath
$receiptDirectory = Split-Path -Parent $ReceiptPath
[System.IO.Directory]::CreateDirectory($readyDirectory) | Out-Null
[System.IO.Directory]::CreateDirectory($receiptDirectory) | Out-Null

$listener = [System.Net.Sockets.TcpListener]::new(
    [System.Net.IPAddress]::Loopback,
    $Port
)
$deadline = [datetime]::UtcNow.AddSeconds($TimeoutSeconds)
$served = @{}
$abandoned = 0
$notFound = 0

try {
    $listener.Start()
    [System.IO.File]::WriteAllText($ReadyPath, "READY 127.0.0.1:$Port`n")

    while ($true) {
        $client = Accept-BeforeDeadline -Listener $listener -Deadline $deadline
        if ($null -eq $client) { break }
        $stream = $null
        try {
            $stream = $client.GetStream()
            $request = try {
                Read-HttpRequest -Stream $stream
            }
            catch [System.IO.IOException] {
                $null
            }
            if ($null -eq $request) {
                $abandoned += 1
                continue
            }
            $path = ($request.Target -split "\?", 2)[0]
            if ($path -eq "/stop") {
                [byte[]] $body = [System.Text.Encoding]::UTF8.GetBytes("stopping")
                Write-HttpResponse -Stream $stream -Status "200 OK" -ContentType "text/plain; charset=utf-8" -Body $body
                break
            }
            if ($pages.ContainsKey($path)) {
                $served[$path] = 1 + [int] $served[$path]
                [byte[]] $body = [System.Text.Encoding]::UTF8.GetBytes($pages[$path])
                Write-HttpResponse -Stream $stream -Status "200 OK" -ContentType "text/html; charset=utf-8" -Body $body
            }
            else {
                $notFound += 1
                [byte[]] $body = [System.Text.Encoding]::UTF8.GetBytes("Not found")
                Write-HttpResponse -Stream $stream -Status "404 Not Found" -ContentType "text/plain; charset=utf-8" -Body $body
            }
        }
        catch {
            $message = $_.Exception.Message
            if ($message -match "transport connection|forcibly closed|connection closed|broken pipe") {
                $abandoned += 1
                continue
            }
            throw
        }
        finally {
            if ($null -ne $stream) { $stream.Dispose() }
            if ($null -ne $client) { $client.Dispose() }
        }
    }

    $lines = @("RESULT ok")
    foreach ($key in ($served.Keys | Sort-Object)) {
        $lines += "served$key=$($served[$key])"
    }
    $lines += "not-found=$notFound"
    $lines += "abandoned-connections=$abandoned"
    [System.IO.File]::WriteAllLines($ReceiptPath, $lines)
}
catch {
    [System.IO.File]::WriteAllLines(
        $ReceiptPath,
        @("RESULT fail", "error=$($_.Exception.Message)")
    )
    throw
}
finally {
    $listener.Stop()
}
