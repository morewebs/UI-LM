# A page in JSON, and the screen it must produce - drawn by `tui --page`, with no recording anywhere.
#
# This is the kit a third party needs: the schema says what a page is (check-schema), and this says what a page means. Each case
# is a directory holding `page.json` and the `screen.txt` it must draw (round 159).
. (Join-Path $PSScriptRoot "utf8.ps1")
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
    # An empty case passes against an empty rendering: a page that draws nothing would be written down as correct (round 163).
    if ($expected.Trim().Length -eq 0) {
        $failures += ("{0}: the case expects an empty screen, so it proves nothing" -f $case.Name)
        continue
    }
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
# The same page, drawn as a document and read back: a case holds `document.txt` for the HTML path as well as `screen.txt` for the
# terminal one, so a runtime that renders HTML is held to the same page (round 165).
foreach ($case in $cases) {
    $page = Join-Path $case.FullName "page.json"
    $wantDoc = Join-Path $case.FullName "document.txt"
    if (-not (Test-Path $wantDoc)) { continue }
    $html = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-conf-" + [guid]::NewGuid().ToString("N") + ".html")
    & $capture render --page $page --out $html 2>&1 | Out-Null
    if (-not (Test-Path $html)) { $failures += ("{0}: the document renderer wrote nothing" -f $case.Name); continue }
    $back = (@(& $capture read --html $html 2>&1) -join "`n")
    # The stored `page.html` is an example a reader may open; if it drifts from what the renderer writes, the example
    # teaches an old document. Line endings are normalised because a checkout may rewrite them.
    $storedDoc = Join-Path $case.FullName "page.html"
    if (Test-Path $storedDoc) {
        $freshText = ((Get-Content -LiteralPath $html -Raw -Encoding UTF8) -replace "`r`n", "`n")
        $storedText = ((Get-Content -LiteralPath $storedDoc -Raw -Encoding UTF8) -replace "`r`n", "`n")
        # `-cne`: PowerShell's `-ne` is case-insensitive, and a class name's case is part of the document.
        if ($freshText -cne $storedText) {
            $failures += ("{0}: the stored page.html differs from a fresh render" -f $case.Name)
        }
    }
    Remove-Item -LiteralPath $html -Force -ErrorAction SilentlyContinue
    $expected = (Get-Content -LiteralPath $wantDoc -Raw -Encoding UTF8).TrimEnd("`n")
    if ($back.TrimEnd("`n") -ne $expected) {
        $a = $back -split "`n"; $b = $expected -split "`n"
        $first = 0
        while ($first -lt $a.Count -and $first -lt $b.Count -and $a[$first] -eq $b[$first]) { $first++ }
        $failures += ("{0}: document line {1} differs - '{2}' against '{3}'" -f $case.Name, ($first + 1), $a[$first], $b[$first])
    }
}

# A kit that omits a kind teaches a runtime nothing about it: every kind of the vocabulary must appear in at least one case
# (round 164).
$seen = @{}
foreach ($case in $cases) {
    $body = Get-Content -LiteralPath (Join-Path $case.FullName "page.json") -Raw -Encoding UTF8
    foreach ($m in [regex]::Matches($body, '"component":\s*"(\w+)"')) { $seen[$m.Groups[1].Value] = $true }
}
$missingKinds = @("Block", "ToolCall", "Thought", "Meter", "Diff", "Frame", "Table" | Where-Object { -not $seen.ContainsKey($_) })
if ($missingKinds.Count -gt 0) {
    Write-Host ("  no conformance case holds: " + ($missingKinds -join ", "))
    Write-Host "the kit does not show a runtime every kind"
    exit 1
}
# Every loop above adds to `$failures`, and the report at line 36 runs before two of them - so the verdict has to be asked
# again here, or a document finding is written down and then ignored (round 165).
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a page did not draw what it is defined by"
    exit 1
}
Write-Host ("every conformance page draws its screen: {0} case(s)" -f $cases.Count)
exit 0
