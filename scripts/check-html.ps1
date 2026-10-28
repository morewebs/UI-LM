# Checks that the browser document holds every line the page holds.
#
# The HTML renderer has unit tests on hand-written pages, and `check-live-page` proves the *viewer*
# serves something. Neither answers the question a person asks when they open the file: is the text
# on screen the text the page recorded? A page can be perfect and the document can drop a block —
# an unescaped character, a component kind the renderer forgot — and every other check would still
# pass.
#
#   powershell -File scripts/check-html.ps1
#
# Exit code 1 if any line of any page is missing from its document.

param(
    [string[]]$Roots = @("eval", "cases"),
    [int]$Limit = 0
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$sessions = @()
foreach ($root in $Roots) {
    $full = if ([System.IO.Path]::IsPathRooted($root)) { $root } else { Join-Path $repo $root }
    if (-not (Test-Path $full)) { continue }
    $sessions += Get-ChildItem -LiteralPath $full -Recurse -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName "session.json") }
}
if ($Limit -gt 0) { $sessions = $sessions | Select-Object -First $Limit }
if ($sessions.Count -eq 0) {
    Write-Host "no recordings found, so nothing was exercised"
    exit 1
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-html-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null

function Visible([string]$html) {
    # The document holds markup inside its lines — a coloured span around a commit hash, `&#39;` for
    # an apostrophe. Comparing raw HTML to screen text therefore fails on lines that are perfectly
    # present, so both sides are reduced to the text a person would read.
    $text = $html -replace '<[^>]+>', ''
    $text = $text -replace '&#39;', "'" -replace '&quot;', '"' -replace '&lt;', '<' -replace '&gt;', '>' -replace '&amp;', '&'
    return $text
}

$lines = 0
$missing = @()
foreach ($session in $sessions) {
    $screen = @(& $capture tui --session $session.FullName --seq 0 2>$null)
    if ($screen.Count -eq 0) { continue }
    $out = Join-Path $work "page.html"
    & $capture render --session $session.FullName --seq 0 --out $out 2>&1 | Out-Null
    if (-not (Test-Path $out)) {
        $missing += ("  " + $session.Name + " -> no document was written")
        continue
    }
    # The document is UTF-8. Read as the console default, a box character arrives mangled and every
    # line holding one looks missing — which is what an earlier version of this check reported.
    $document = Visible (Get-Content -LiteralPath $out -Raw -Encoding UTF8)
    foreach ($line in $screen) {
        $text = $line.Trim()
        # Very short fragments ("│", "--") are noise; a lost word is not.
        if ($text.Length -lt 4) { continue }
        $lines++
        if (-not $document.Contains($text)) {
            $missing += ("  " + $session.Name + " -> missing: " + $text.Substring(0, [Math]::Min(60, $text.Length)))
        }
    }
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
Write-Host ("pages checked: {0}, lines checked: {1}" -f $sessions.Count, $lines)
if ($missing.Count -gt 0) {
    $missing | Select-Object -First 10 | ForEach-Object { Write-Host $_ }
    Write-Host ("{0} line(s) never reached the document" -f $missing.Count)
    exit 1
}
Write-Host "every line the page holds reached the document"
