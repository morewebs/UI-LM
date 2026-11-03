# A page that offers a command the viewer refuses is a dead button.
#
# Every half of this loop was tested alone: the renderer's test covers the button carrying `data-binding`, the viewer's
# tests cover `/keys`, and nothing joined them. The renderer emitted no `data-binding` at all for two rounds, and no test
# noticed because no test asked the viewer what it thought of the page (round 41).
#
# This check serves a recording that holds shortcuts, reads the bindings out of the served page, and posts each one back.
# A binding the viewer cannot translate is a button that does nothing.

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { $capture = Join-Path $repo "target\release\ui-lm-capture.exe" }
$serve = Join-Path $repo "target\debug\ui-lm-serve.exe"
if (-not (Test-Path $serve)) { $serve = Join-Path $repo "target\release\ui-lm-serve.exe" }
if (-not (Test-Path $capture) -or -not (Test-Path $serve)) {
    Write-Host "no binaries, so nothing was measured"
    exit 1
}

# A recording whose pages hold shortcuts: the Grok startup card.
$session = Join-Path $repo "eval\unseen\unseen-listing"
if (-not (Test-Path $session)) {
    Write-Host "the shortcut-bearing recording is not on this machine, so the loop was not measured"
    exit 1
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-loop-" + [guid]::NewGuid().ToString("N"))
$control = Join-Path $work "keys.txt"
$port = 8796
$failures = @()

try {
    New-Item -ItemType Directory -Force -Path $work | Out-Null
    New-Item -ItemType File -Path $control -Force | Out-Null

    $viewer = Start-Process -FilePath $serve -PassThru -NoNewWindow -ArgumentList @(
        "--session", $session, "--keys", $control, "--port", "$port")
    Start-Sleep -Seconds 3

    # The bindings are read from the **rendered document**, not from the live page: the viewer shows the latest screen,
    # and the startup card that carries the shortcuts is two frames back. What this check joins is the renderer's
    # `data-binding` and the viewer's `/keys`, which is the pair nothing had tested together (round 42).
    $htmlPath = Join-Path $work "page.html"
    & $capture render --session $session --all --out $htmlPath 2>&1 | Out-Null
    $page = if (Test-Path $htmlPath) { Get-Content -LiteralPath $htmlPath -Raw -Encoding UTF8 } else { "" }
    if (-not $page) { $failures += "the renderer wrote no document" }
    try { $null = Invoke-WebRequest -Uri "http://127.0.0.1:$port/" -UseBasicParsing -TimeoutSec 5 } catch { $failures += "the viewer did not serve a page" }

    # Wait for the process, not just the signal: a viewer killed a moment ago can still hold its socket, and starting
    # the next one on that port failed under the whole suite with a Win32Exception (round 122).
    if (-not $viewer.HasExited) { $viewer.Kill(); $viewer.WaitForExit(5000) | Out-Null }

    $bindings = @([regex]::Matches($page, 'data-binding="([^"]+)"') | ForEach-Object { $_.Groups[1].Value } | Select-Object -Unique)
    if ($bindings.Count -eq 0) {
        $failures += "the served page offers no binding, so there is nothing to press"
    } else {
        # The viewer is restarted for the posts: a binding is written to the control file, and this check only needs to
        # know the viewer accepts it.
        $viewer = Start-Process -FilePath $serve -PassThru -NoNewWindow -ArgumentList @(
            "--session", $session, "--keys", $control, "--port", "$($port + 1)")
        Start-Sleep -Seconds 3
        $refused = @()
        foreach ($binding in $bindings) {
            $encoded = [uri]::EscapeDataString($binding)
            try {
                $response = Invoke-WebRequest -Uri "http://127.0.0.1:$($port + 1)/keys?binding=$encoded" -Method POST -UseBasicParsing -TimeoutSec 5
                if ($response.StatusCode -lt 200 -or $response.StatusCode -ge 300) { $refused += $binding }
            } catch {
                $refused += $binding
            }
        }
        if (-not $viewer.HasExited) { $viewer.Kill() }
        if ($refused.Count -gt 0) {
            $failures += ("the page offers {0} binding(s) and the viewer refuses {1}: {2}" -f $bindings.Count, $refused.Count, ($refused -join ", "))
        } else {
            Write-Host ("the loop closes: {0} binding(s) on the page, all accepted by the viewer" -f $bindings.Count)
        }
    }
} finally {
    Get-Process -Name "ui-lm-serve" -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "*$repo*" } | ForEach-Object { $_.Kill() }
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a button on the page cannot reach the program"
    exit 1
}
exit 0
