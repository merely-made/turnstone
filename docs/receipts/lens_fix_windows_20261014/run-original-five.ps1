$repo = 'C:/Users/mark_/Code/worktrees/turnstone-win-qual'
$runner = "$repo/docs/receipts/lens_fix_windows_20261014/run-native-scenario.ps1"
$scratch = $PSScriptRoot
$serveLog = Join-Path $scratch 'serve.out'
$serveErr = Join-Path $scratch 'serve.err'
$server = Start-Process -FilePath python -PassThru -NoNewWindow `
    -ArgumentList @("$repo/scenarios/fixtures/document_a11y/serve.py",
                    '--certificate', "$scratch/a11ycert/cert.pem", '--key', "$scratch/a11ycert/key.pem") `
    -RedirectStandardOutput $serveLog -RedirectStandardError $serveErr
try {
    $deadline = (Get-Date).AddSeconds(20)
    while (-not ((Test-Path $serveLog) -and (Select-String -Path $serveLog -Pattern 'READER_PORT' -Quiet))) {
        if ((Get-Date) -gt $deadline -or $server.HasExited) { throw "fixture server did not start: $(Get-Content $serveErr -Raw)" }
        Start-Sleep -Milliseconds 200
    }
    foreach ($line in Get-Content $serveLog) {
        if ($line -match '^(TURNSTONE_A11Y_\w+)=(.+)$') { Set-Item "Env:$($Matches[1])" $Matches[2]; Write-Output "env $line" }
    }
    $runs = @(
        @('document_a11y_gemini','fixed-five-gemini'),
        @('document_a11y_gopher','fixed-five-gopher'),
        @('document_a11y_micron','fixed-five-micron'),
        @('document_a11y_lens','fixed-five-lens'),
        @('reader_appearance','fixed-five-reader')
    )
    foreach ($r in $runs) {
        $runArgs = @('-NoProfile','-File',$runner,'-Scenario',$r[0],'-Case',$r[1])
        $out = & pwsh @runArgs 2>&1 | Out-String
        Write-Output ("{0} {1} exit {2}: {3}" -f $r[1], $r[0], $LASTEXITCODE, (($out.Trim() -split "`n" | Select-Object -Last 2) -join ' | '))
    }
} finally {
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}
