param(
    [Parameter(Mandatory = $true)]
    [int] $Port,

    [Parameter(Mandatory = $true)]
    [string] $ReadyPath,

    [Parameter(Mandatory = $true)]
    [string] $ReceiptPath,

    [int] $TimeoutSeconds = 240
)

$ErrorActionPreference = "Stop"

function Read-HttpRequest {
    param([System.IO.Stream] $Stream)

    $bytes = [System.Collections.Generic.List[byte]]::new()
    [byte[]] $terminator = @(13, 10, 13, 10)
    $matched = 0
    while ($bytes.Count -le 32768 -and $matched -lt $terminator.Length) {
        $current = $Stream.ReadByte()
        if ($current -lt 0) {
            # Chromium may open and abandon a speculative connection before it
            # sends a request. That is transport noise, not a fixture failure.
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
    $headers = @{}
    foreach ($line in $lines[1..($lines.Count - 1)]) {
        if ([string]::IsNullOrEmpty($line)) {
            continue
        }
        $pair = $line -split ":", 2
        if ($pair.Count -eq 2) {
            $headers[$pair[0].Trim().ToLowerInvariant()] = $pair[1].Trim()
        }
    }
    [pscustomobject]@{
        Method = $requestParts[0]
        Target = $requestParts[1]
        Headers = $headers
    }
}

function Write-HttpResponse {
    param(
        [System.IO.Stream] $Stream,
        [string] $Status,
        [string] $ContentType,
        [byte[]] $Body,
        [hashtable] $Headers = @{}
    )

    $head = [System.Text.StringBuilder]::new()
    [void] $head.Append("HTTP/1.1 $Status`r`n")
    [void] $head.Append("Content-Type: $ContentType`r`n")
    [void] $head.Append("Content-Length: $($Body.Length)`r`n")
    [void] $head.Append("Cache-Control: no-store`r`n")
    [void] $head.Append("Connection: close`r`n")
    foreach ($entry in $Headers.GetEnumerator()) {
        [void] $head.Append("$($entry.Key): $($entry.Value)`r`n")
    }
    [void] $head.Append("`r`n")
    [byte[]] $headBytes = [System.Text.Encoding]::ASCII.GetBytes($head.ToString())
    $Stream.Write($headBytes, 0, $headBytes.Length)
    $Stream.Write($Body, 0, $Body.Length)
    $Stream.Flush()
}

function Accept-BeforeDeadline {
    param(
        [System.Net.Sockets.TcpListener] $Listener,
        [datetime] $Deadline
    )

    while (-not $Listener.Pending()) {
        if ([datetime]::UtcNow -ge $Deadline) {
            throw "timed out waiting for the browser decisions receipt"
        }
        Start-Sleep -Milliseconds 20
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
$permissionResponses = 0
$permissionResults = 0
$abandonedConnections = 0
$completed = $false

try {
    $listener.Start()
    [System.IO.File]::WriteAllText($ReadyPath, "READY 127.0.0.1:$Port`n")

    while (-not $completed) {
        $client = Accept-BeforeDeadline -Listener $listener -Deadline $deadline
        $stream = $null
        try {
            $stream = $client.GetStream()
            $request = Read-HttpRequest -Stream $stream
            if ($null -eq $request) {
                $abandonedConnections += 1
                continue
            }
            $path = ($request.Target -split "\?", 2)[0]
            if ($path -eq "/permission") {
                $permissionResponses += 1
                $html = @"
<!doctype html>
<meta charset="utf-8">
<title>Turnstone D0 permission fixture</title>
<style>
html, body { margin: 0; min-height: 100%; font: 24px system-ui; background: #f4efe3; }
button { position: fixed; inset: 0; width: 100%; height: 100%; border: 0; color: #172033; background: #f4efe3; font: inherit; }
</style>
<button id="request">Request location permission</button>
<script>
const next = () => setTimeout(() => location.href = '/permission-result', 80);
document.querySelector('#request').addEventListener('click', () => {
  try {
    navigator.geolocation.getCurrentPosition(next, next);
  } catch (_) {
    next();
  }
});
</script>
"@
                [byte[]] $body = [System.Text.Encoding]::UTF8.GetBytes($html)
                Write-HttpResponse -Stream $stream -Status "200 OK" -ContentType "text/html; charset=utf-8" -Body $body
            }
            elseif ($path -eq "/permission-result") {
                $permissionResults += 1
                [byte[]] $body = [System.Text.Encoding]::UTF8.GetBytes(
                    "<!doctype html><meta charset=`"utf-8`"><title>Permission answered</title><style>html,body{margin:0;width:100%;height:100%;font:24px system-ui;background:#f4efe3}body{display:grid;place-items:center;color:#172033}h1{margin:60px 16px 16px;font-size:24px;line-height:1.3;text-align:center}</style><h1>Permission callback answered</h1>"
                )
                Write-HttpResponse -Stream $stream -Status "200 OK" -ContentType "text/html; charset=utf-8" -Body $body
                $completed = $true
            }
            else {
                [byte[]] $body = [System.Text.Encoding]::UTF8.GetBytes("Not found")
                Write-HttpResponse -Stream $stream -Status "404 Not Found" -ContentType "text/plain; charset=utf-8" -Body $body
            }
        }
        catch {
            $message = $_.Exception.Message
            if ($message -match "transport connection|forcibly closed|connection closed|broken pipe") {
                # Chromium also resets speculative or superseded requests. Keep
                # serving until the authenticated response itself succeeds.
                $abandonedConnections += 1
                continue
            }
            throw
        }
        finally {
            if ($null -ne $stream) { $stream.Dispose() }
            if ($null -ne $client) { $client.Dispose() }
        }
    }

    if ($permissionResponses -lt 1 -or $permissionResults -lt 1) {
        throw "fixture did not observe both permission-page and callback-result traffic"
    }
    [System.IO.File]::WriteAllLines(
        $ReceiptPath,
        @(
            "RESULT ok",
            "permission-page-responses=$permissionResponses",
            "permission-result-responses=$permissionResults",
            "abandoned-connections=$abandonedConnections",
            "sensitive-values-recorded=false"
        )
    )
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
