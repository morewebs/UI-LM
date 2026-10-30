# Checks that every document containing panes draws them side by side.
#
# `check-html` proves the words and the `data-layout="row"` marker, and it renders frames 0 and 20 of every recording
# — neither of which is guaranteed to hold a split. That is why three attempts to put a stylesheet assertion inside it
# could not be made to fail: the document they examined often had no `.panes` container at all.
#
# This walks every recording, asks each one which frame holds a split (`frames` answers that), renders exactly that
# frame, and reads three facts from the document:
#
#   * the container exists and declares the row layout;
#   * the stylesheet rule for it really lays two tracks out in a row — not merely that the document contains the
#     words `display: grid` and `grid-template-columns` somewhere;
#   * a pane-less frame of the same recording is drawn with no container at all, so the marker is not sprayed
#     everywhere.
#
#   powershell -File scripts/check-layout-all.ps1
#
# Exit code 1 if any paned page is not drawn side by side, or if no recording holds a split to examine.
#
# Three earlier attempts at this failed for one reason worth recording: the regex that finds a paned frame was written
# through a Python string, which doubled the backslashes, so `\s` became `\\s` and matched a literal backslash. Every
# attempt reported "no recording holds a frame with two panes" — true of the pattern, not of the corpus. The pattern is
# written literally here.

param(
    [string[]]$Roots = @("eval", "cases")
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
if ($sessions.Count -eq 0) {
    Write-Host "no recordings found, so nothing was rendered"
    exit 1
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-layout-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null

$panedFinder = [regex]'^\s*frame\s+(\d+)\s+.*?\d+\s+panes'
$singleFinder = [regex]'^\s*frame\s+(\d+)\s+.*?one pane'

$problems = @()
$panedRecordings = 0
$rendered = 0
foreach ($session in $sessions) {
    $panedFrame = $null
    foreach ($line in (& $capture frames --session $session.FullName 2>$null)) {
        if ($panedFinder.IsMatch($line)) {
            $panedFrame = [int]$panedFinder.Match($line).Groups[1].Value
            break
        }
    }
    if ($null -eq $panedFrame) { continue }
    $panedRecordings++
    $label = "  " + $session.Name + " (frame " + $panedFrame + ")"

    $document = Join-Path $work ("paned-" + $rendered + ".html")
    & $capture render --session $session.FullName --seq $panedFrame --out $document 2>&1 | Out-Null
    if (-not (Test-Path $document)) {
        $problems += ($label + ": the paned frame produced no document")
        continue
    }
    $rendered++

    # UTF-8: read as the console default, a box character arrives mangled and the document looks empty.
    $html = Get-Content -LiteralPath $document -Raw -Encoding UTF8
    if ($html -notmatch '<div class="panes"') { $problems += ($label + ": no pane container") }
    if ($html -notmatch 'data-layout="row"') { $problems += ($label + ": does not declare a row layout") }
    $wrappers = ([regex]::Matches($html, '<div class="pane">')).Count
    if ($wrappers -lt 2) { $problems += ($label + ": only " + $wrappers + " pane div(s)") }

    # The stylesheet, read as **one rule**: an earlier version searched the whole document and was satisfied by a
    # `display: grid` in one rule and a `grid-template-columns` in another.
    $rules = [regex]::Matches($html, '(?s)\.panes\s*\{([^}]*)\}')
    $laysOutInRows = @($rules | Where-Object {
            $body = $_.Groups[1].Value
            ($body -match 'display:\s*(grid|flex)') -and ($body -match 'grid-template-columns:\s*[^;}]*\s[^;}]*')
        }).Count -gt 0
    if (-not $laysOutInRows) { $problems += ($label + ": its stylesheet does not lay two tracks out in a row") }

    # The other direction: a frame of the same recording with no split must carry no container.
    $singleFrame = $null
    foreach ($line in (& $capture frames --session $session.FullName 2>$null)) {
        if ($singleFinder.IsMatch($line)) {
            $singleFrame = [int]$singleFinder.Match($line).Groups[1].Value
            break
        }
    }
    if ($null -ne $singleFrame) {
        $singleDoc = Join-Path $work ("single-" + $rendered + ".html")
        & $capture render --session $session.FullName --seq $singleFrame --out $singleDoc 2>&1 | Out-Null
        if (Test-Path $singleDoc) {
            $singleHtml = Get-Content -LiteralPath $singleDoc -Raw -Encoding UTF8
            if ($singleHtml -match '<div class="panes"') {
                $problems += ($label + ": a frame with no split was drawn with a pane container")
            }
        }
    }
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue

if ($panedRecordings -eq 0) {
    Write-Host "no recording holds a frame with two panes, so a layout check has nothing to examine"
    exit 1
}
if ($problems.Count -gt 0) {
    $problems | Select-Object -First 10 | ForEach-Object { Write-Host $_ }
    Write-Host "a paned page was not drawn as panes side by side"
    exit 1
}
Write-Host ("every paned page is drawn side by side: {0} recording(s), {1} document(s)" -f $panedRecordings, $rendered)
exit 0
