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

# W6c page-text acceptance fixture. Serves two articles whose BODIES carry a
# distinctive word that appears in neither the URL nor the <title>, so a recall
# that finds them can only have read the extracted main text:
#
#   /notes/7   body word HAGIOSCOPE, title "Field notes", path /notes/7
#   /notes/8   body word PARBUCKLE, title "More field notes", path /notes/8
#
# Each body is real prose so fleece's readability pass accepts it as an
# article. Nothing here runs script and nothing is timing-dependent.

$ErrorActionPreference = "Stop"

function Read-HttpRequest {
    param([System.IO.Stream] $Stream)

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

$reset = "html,body{margin:0;padding:0;border:0}body{font:16px/1.2 system-ui,sans-serif;color:#172033;background:#f4efe3}p{margin:0 0 16px}"

$pages = @{}

$pages["/notes/7"] = @"
<!doctype html>
<meta charset="utf-8">
<title>Field notes</title>
<style>$reset article{max-width:40em;margin:0 auto;padding:24px}</style>
<nav>Index Archive About</nav>
<article>
<h1>Looking through the wall</h1>
<p>A HAGIOSCOPE is the slanted opening cut through a thick church wall so that
someone standing in a side chapel can still see the altar. It exists because
the building was made of load-bearing stone and the sightline was worth more
than the masonry it cost.</p>
<p>The interesting part is that the opening is not a window in any ordinary
sense. It frames one fixed thing from one fixed place, and it is useless from
anywhere else in the room. A view that only works from where you are standing
is a strange thing to build into a wall on purpose.</p>
<p>Nothing about the shape of the hole tells you what it was for. You have to
stand in the right corner and look. Then the whole geometry of the wall
resolves at once into an argument about who was allowed to see what.</p>
</article>
<footer>Filed under masonry</footer>
"@

$pages["/notes/8"] = @"
<!doctype html>
<meta charset="utf-8">
<title>More field notes</title>
<style>$reset article{max-width:40em;margin:0 auto;padding:24px}</style>
<nav>Index Archive About</nav>
<article>
<h1>Rolling a barrel uphill</h1>
<p>To PARBUCKLE a cask is to roll it up a ramp on a doubled rope, both ends
held at the top, the bight passed under the barrel. Pulling on the free ends
turns the rope into a sling that gains ground twice for every arm's length
hauled.</p>
<p>It is a purchase made out of geometry rather than machinery. There is no
block, no gear, and nothing to break; the mechanical advantage is entirely in
how the line is led, and it is recovered the instant the load is landed.</p>
<p>The technique survives in salvage work for exactly that reason. A rope and
a slope are available anywhere, and a method that needs nothing else is the
one that is still there when the crane is not.</p>
</article>
<footer>Filed under rigging</footer>
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
