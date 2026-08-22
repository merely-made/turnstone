param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("GeminiImage", "GeminiDownload", "Titan", "Spartan")]
    [string] $Mode,

    [Parameter(Mandatory = $true)]
    [int] $Port,

    [Parameter(Mandatory = $true)]
    [string] $ReadyPath,

    [Parameter(Mandatory = $true)]
    [string] $ReceiptPath,

    [int] $TimeoutSeconds = 120
)

$ErrorActionPreference = "Stop"

function Read-CrlfPacket {
    param([System.IO.Stream] $Stream)

    $lineBytes = [System.Collections.Generic.List[byte]]::new()
    $previous = -1
    while ($lineBytes.Count -le 4096) {
        $current = $Stream.ReadByte()
        if ($current -lt 0) {
            throw "connection closed before the request line"
        }
        $lineBytes.Add([byte] $current)
        if ($previous -eq 13 -and $current -eq 10) {
            break
        }
        $previous = $current
    }
    if ($lineBytes.Count -gt 4096) {
        throw "request line exceeded 4096 bytes"
    }

    $line = [System.Text.Encoding]::UTF8.GetString(
        $lineBytes.ToArray(),
        0,
        $lineBytes.Count - 2
    )
    [pscustomobject]@{ Line = $line; Stream = $Stream }
}

function Read-ExactBody {
    param(
        [System.IO.Stream] $Stream,
        [int] $Length
    )

    [byte[]] $body = [byte[]]::new($Length)
    $offset = 0
    while ($offset -lt $Length) {
        $count = $Stream.Read($body, $offset, $Length - $offset)
        if ($count -eq 0) {
            throw "connection closed after $offset of $Length body bytes"
        }
        $offset += $count
    }
    $body
}

function Write-Response {
    param(
        [System.IO.Stream] $Stream,
        [string] $Text
    )

    [byte[]] $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
    $Stream.Write($bytes, 0, $bytes.Length)
    $Stream.Flush()
}

function Open-TlsServerStream {
    param(
        [System.Net.Sockets.TcpClient] $Client,
        [System.Security.Cryptography.X509Certificates.X509Certificate2] $Certificate
    )

    $tls = [System.Net.Security.SslStream]::new($Client.GetStream(), $false)
    $options = [System.Net.Security.SslServerAuthenticationOptions]::new()
    $options.ServerCertificate = $Certificate
    $options.ClientCertificateRequired = $false
    $options.EnabledSslProtocols =
        [System.Security.Authentication.SslProtocols]::Tls12 -bor
        [System.Security.Authentication.SslProtocols]::Tls13
    $options.CertificateRevocationCheckMode =
        [System.Security.Cryptography.X509Certificates.X509RevocationMode]::NoCheck
    $tls.AuthenticateAsServer($options)
    $tls
}

function Accept-BeforeDeadline {
    param(
        [System.Net.Sockets.TcpListener] $Listener,
        [datetime] $Deadline
    )

    while (-not $Listener.Pending()) {
        if ([datetime]::UtcNow -ge $Deadline) {
            throw "timed out waiting for a $Mode connection"
        }
        Start-Sleep -Milliseconds 20
    }
    $Listener.AcceptTcpClient()
}

function Parse-SpartanRequest {
    param([System.Net.Sockets.TcpClient] $Client)

    $stream = $Client.GetStream()
    $packet = Read-CrlfPacket -Stream $stream
    $parts = $packet.Line -split " ", 3
    if ($parts.Count -ne 3) {
        throw "bad Spartan request line: $($packet.Line)"
    }
    $length = 0
    if (-not [int]::TryParse($parts[2], [ref] $length) -or $length -lt 0) {
        throw "bad Spartan body length: $($parts[2])"
    }
    [pscustomobject]@{
        Host = $parts[0]
        Path = $parts[1]
        Length = $length
        Body = Read-ExactBody -Stream $stream -Length $length
        Stream = $stream
    }
}

$receiptDirectory = Split-Path -Parent $ReceiptPath
$readyDirectory = Split-Path -Parent $ReadyPath
[System.IO.Directory]::CreateDirectory($receiptDirectory) | Out-Null
[System.IO.Directory]::CreateDirectory($readyDirectory) | Out-Null

$listener = [System.Net.Sockets.TcpListener]::new(
    [System.Net.IPAddress]::Loopback,
    $Port
)
$deadline = [datetime]::UtcNow.AddSeconds($TimeoutSeconds)
$receipt = [System.Collections.Generic.List[string]]::new()

try {
    $listener.Start()
    [System.IO.File]::WriteAllText($ReadyPath, "READY $Mode 127.0.0.1:$Port`n")

    if ($Mode -in @("GeminiImage", "GeminiDownload", "Titan")) {
        $rsa = [System.Security.Cryptography.RSA]::Create(2048)
        $certificateRequest = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(
            "CN=127.0.0.1",
            $rsa,
            [System.Security.Cryptography.HashAlgorithmName]::SHA256,
            [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
        )
        $san = [System.Security.Cryptography.X509Certificates.SubjectAlternativeNameBuilder]::new()
        $san.AddIpAddress([System.Net.IPAddress]::Loopback)
        $certificateRequest.CertificateExtensions.Add($san.Build())
        $ephemeralCertificate = $certificateRequest.CreateSelfSigned(
            [datetimeoffset]::UtcNow.AddMinutes(-5),
            [datetimeoffset]::UtcNow.AddHours(1)
        )
        # Windows SslStream cannot serve the ephemeral private key returned by
        # CreateSelfSigned. A PFX round trip gives Schannel a persisted user-key
        # container while keeping the short-lived receipt certificate off disk.
        $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
            $ephemeralCertificate.Export(
                [System.Security.Cryptography.X509Certificates.X509ContentType]::Pfx
            ),
            "",
            [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::UserKeySet -bor
                [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::PersistKeySet -bor
                [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::Exportable
        )
        $ephemeralCertificate.Dispose()

        $client = Accept-BeforeDeadline -Listener $listener -Deadline $deadline
        try {
            $tls = Open-TlsServerStream -Client $client -Certificate $certificate
            try {
                $packet = Read-CrlfPacket -Stream $tls
                if ($Mode -eq "Titan") {
                    $fields = $packet.Line -split ";"
                    $target = $fields[0]
                    $parameters = @{}
                    foreach ($field in $fields[1..($fields.Count - 1)]) {
                        $pair = $field -split "=", 2
                        if ($pair.Count -eq 2) {
                            $parameters[$pair[0]] = $pair[1]
                        }
                    }
                    $size = 0
                    if (-not [int]::TryParse($parameters["size"], [ref] $size)) {
                        throw "Titan request omitted a numeric size"
                    }
                    $body = Read-ExactBody -Stream $tls -Length $size
                    $bodyText = [System.Text.Encoding]::UTF8.GetString($body)

                    $expectedTarget = "titan://127.0.0.1:$Port/upload"
                    if ($target -ne $expectedTarget) {
                        throw "Titan target was '$target', expected '$expectedTarget'"
                    }
                    if ($bodyText -ne "# Acceptance draft`n") {
                        throw "Titan body did not match the staged acceptance body"
                    }
                    if ($parameters["mime"] -ne "text/plain") {
                        throw "Titan MIME was '$($parameters['mime'])'"
                    }
                    if ($parameters["token"] -ne "acceptance-token") {
                        throw "Titan token did not match the hidden staged token"
                    }

                    Write-Response -Stream $tls -Text "20 text/gemini`r`n# Titan accepted`n"
                    $receipt.Add("RESULT ok")
                    $receipt.Add("protocol=titan")
                    $receipt.Add("target=$target")
                    $receipt.Add("body-bytes=$size")
                    $receipt.Add("mime=text/plain")
                    $receipt.Add("token-present=true")
                    $receipt.Add("response=20")
                }
                elseif ($Mode -eq "GeminiDownload") {
                    $expectedTarget = "gemini://127.0.0.1:$Port/archive.bin"
                    if ($packet.Line -ne $expectedTarget) {
                        throw "Gemini download target was '$($packet.Line)', expected '$expectedTarget'"
                    }
                    [byte[]] $header = [System.Text.Encoding]::ASCII.GetBytes(
                        "20 application/octet-stream`r`n"
                    )
                    [byte[]] $payload = @(0, 1, 2, 255, 84, 117, 114, 110, 115, 116, 111, 110, 101)
                    $tls.Write($header, 0, $header.Length)
                    $tls.Write($payload, 0, $payload.Length)
                    $tls.Flush()

                    $receipt.Add("RESULT ok")
                    $receipt.Add("protocol=gemini")
                    $receipt.Add("target=$expectedTarget")
                    $receipt.Add("response=20 application/octet-stream")
                    $receipt.Add("body-bytes=$($payload.Length)")
                }
                else {
                    $expectedTarget = "gemini://127.0.0.1:$Port/inline-image.gmi"
                    if ($packet.Line -ne $expectedTarget) {
                        throw "Gemini page target was '$($packet.Line)', expected '$expectedTarget'"
                    }
                    Write-Response -Stream $tls -Text (
                        "20 text/gemini`r`n" +
                        "# Inline image acceptance`n" +
                        "The colored image below arrived as a second Gemini request.`n" +
                        "=> /acceptance-image.png Linked acceptance image`n" +
                        "## Image loaded`n"
                    )
                }
                $tls.ShutdownAsync().GetAwaiter().GetResult()
            }
            finally {
                if ($null -ne $tls) { $tls.Dispose() }
            }
        }
        finally {
            if ($null -ne $client) { $client.Dispose() }
        }

        if ($Mode -eq "GeminiImage") {
            $imageClient = Accept-BeforeDeadline -Listener $listener -Deadline $deadline
            try {
                $imageTls = Open-TlsServerStream -Client $imageClient -Certificate $certificate
                try {
                    $imagePacket = Read-CrlfPacket -Stream $imageTls
                    $expectedImage = "gemini://127.0.0.1:$Port/acceptance-image.png"
                    if ($imagePacket.Line -ne $expectedImage) {
                        throw "Gemini image target was '$($imagePacket.Line)', expected '$expectedImage'"
                    }
                    [byte[]] $header = [System.Text.Encoding]::ASCII.GetBytes("20 image/png`r`n")
                    [byte[]] $image = [Convert]::FromBase64String(
                        "iVBORw0KGgoAAAANSUhEUgAAAEAAAAAoCAYAAABOzvzpAAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAFbSURBVGhD5dDNbYZAEINh6khnqS2pI6f0lmgiWeJ7BWNYdoefWHoOaOdgPL19f/7ME9+VPt6/TjW9/P0sLDoKC1VbHSDCsiOwULUpSriwdE8sVO1vAHFh+R5YqNrLAJKFt0exULXFAYIL71uxULXVASQLb1uwUDU7QHDh/R4sVG3TAOLC+y1YqNquASQLbx0WqtY0QHDh/RoWqtY8gGTh7RIWqnZ4gODC+zkWqtZlAHHhfWChal0HkCy8ZaFqQwYILrpjoWrDBpAs8c5C1YYPEFxYqlLJAOLCchVKB5AsLDjaKQMEFxYd5bQBJAvLjnD6AFHChaV7usQA4sLyPVxqAMnC26MuOUBw4X2ryw4gWXjb4vIDBBfe73GLAcSF91vcagDJwlvnlgMEF96vue0AkoW3S24/QHDh/dwjBhAX3odHDSBZePvIAYKL7h47gGSJ98cPELL8iwFkKb+JgykQp8bUuAAAAABJRU5ErkJggg=="
                    )
                    $imageTls.Write($header, 0, $header.Length)
                    $imageTls.Write($image, 0, $image.Length)
                    $imageTls.Flush()
                    $imageTls.ShutdownAsync().GetAwaiter().GetResult()

                    $receipt.Add("RESULT ok")
                    $receipt.Add("protocol=gemini")
                    $receipt.Add("page=$expectedTarget")
                    $receipt.Add("image=$expectedImage")
                    $receipt.Add("image-bytes=$($image.Length)")
                    $receipt.Add("requests=2")
                    $receipt.Add("response=20 image/png")
                }
                finally {
                    if ($null -ne $imageTls) { $imageTls.Dispose() }
                }
            }
            finally {
                if ($null -ne $imageClient) { $imageClient.Dispose() }
            }
        }
    }
    else {
        $firstClient = Accept-BeforeDeadline -Listener $listener -Deadline $deadline
        try {
            $first = Parse-SpartanRequest -Client $firstClient
            if ($first.Host -ne "127.0.0.1" -or $first.Path -ne "/form" -or $first.Length -ne 0) {
                throw "unexpected Spartan fetch: '$($first.Host) $($first.Path) $($first.Length)'"
            }
            Write-Response -Stream $first.Stream -Text (
                "2 text/gemini`r`n" +
                "=: /submit Submit acceptance body`n"
            )
        }
        finally {
            if ($null -ne $firstClient) { $firstClient.Dispose() }
        }

        $secondClient = Accept-BeforeDeadline -Listener $listener -Deadline $deadline
        try {
            $second = Parse-SpartanRequest -Client $secondClient
            $bodyText = [System.Text.Encoding]::UTF8.GetString($second.Body)
            if ($second.Host -ne "127.0.0.1" -or $second.Path -ne "/submit") {
                throw "unexpected Spartan submit target: '$($second.Host) $($second.Path)'"
            }
            if ($bodyText -ne "Spartan acceptance") {
                throw "Spartan body did not match the staged acceptance body"
            }
            Write-Response -Stream $second.Stream -Text "2 text/gemini`r`n# Spartan accepted`n"

            $receipt.Add("RESULT ok")
            $receipt.Add("protocol=spartan")
            $receipt.Add("fetch=127.0.0.1 /form 0")
            $receipt.Add("submit=127.0.0.1 /submit $($second.Length)")
            $receipt.Add("body-bytes=$($second.Length)")
            $receipt.Add("response=2")
        }
        finally {
            if ($null -ne $secondClient) { $secondClient.Dispose() }
        }
    }

    [System.IO.File]::WriteAllLines($ReceiptPath, $receipt)
}
catch {
    [System.IO.File]::WriteAllLines(
        $ReceiptPath,
        @("RESULT fail", "error=$($_.Exception.Message)")
    )
    throw
}
finally {
    if ($null -ne $certificate) { $certificate.Dispose() }
    if ($null -ne $rsa) { $rsa.Dispose() }
    $listener.Stop()
}
