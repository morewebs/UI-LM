# A page in JSON, and the screen it must produce - drawn by `tui --page`, with no recording anywhere.
#
# This is the kit a third party needs: the schema says what a page is (check-schema), and this says what a page means. Each case
# is a directory holding `page.json` and the `screen.txt` it must draw (round 159).
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { $capture = Join-Path $repo "target\release\ui-lm-capture.exe" }
if (-not (Test-Path $capture)) { Write-Host "no capture binary, so nothing was measured"; exit 1 }

$root = Join-Path $repo "docs\conformance"
$cases = @(Get-ChildItem -LiteralPath $root -Directory | Where-Object { Test-Path (Join-Path $_.FullName "page.json") })
if ($cases.Count -eq 0) { Write-Host "no conformance case is on this machine, so nothing was measured"; exit 1 }

$failures = @()
foreach ($case in $cases) {
    $page = Join-Path $case.FullName "page.json"
    $want = Join-Path $case.FullName "screen.txt"
    $raw = & $capture tui --page $page 2>&1
    if ($LASTEXITCODE -ne 0) { $failures += ("{0}: the renderer refused the page: {1}" -f $case.Name, ($raw -join " ")); continue }
    $got = (@($raw) -join "`n")
    $expected = Get-Content -LiteralPath $want -Raw -Encoding UTF8
    if ($got.TrimEnd("`n") -ne $expected.TrimEnd("`n")) {
        $first = 0
        $a = $got -split "`n"; $b = $expected -split "`n"
        while ($first -lt $a.Count -and $first -lt $b.Count -and $a[$first] -eq $b[$first]) { $first++ }
        $failures += ("{0}: line {1} differs - the page draws '{2}', the case expects '{3}'" -f $case.Name, ($first + 1), $a[$first], $b[$first])
    }
}
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a page did not draw the screen it is defined by"
    exit 1
}
Write-Host ("every conformance page draws its screen: {0} case(s)" -f $cases.Count)
exit 0
