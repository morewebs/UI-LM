# Checks that a document containing panes actually draws them side by side.
#
# `check-html` proves the words and the `data-layout="row"` marker, and it renders frames 0 and 20 of every recording
# — neither of which is guaranteed to hold a split. That is why three attempts to put a stylesheet assertion inside it
# could not be made to fail: the document they examined often had no `.panes` container at all.
#
# So this check needs no sampling. It asks a recording which frame holds a split (the `frames` command answers that),
# renders exactly that frame, and reads the document:
#
#   * the container exists and declares the row layout;
#   * the stylesheet rule for it really lays two tracks out in a row — not merely that the document contains the
#     words `display: grid` and `grid-template-columns` somewhere;
#   * a pane-less document has no `.panes` container at all, so the marker is not sprayed everywhere.
#
#   powershell -File scripts/check-layout.ps1
#
# Exit code 1 if a paned page is not drawn side by side, or if the check cannot find anything to examine.

param(
    [string]$Session = "cases/hard/pane-resize"
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$session = if ([System.IO.Path]::IsPathRooted($Session)) { $Session } else { Join-Path $repo $Session }
if (-not (Test-Path (Join-Path $session "session.json"))) {
    Write-Host ("no recording at {0}, so nothing was rendered" -f $Session)
    exit 1
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-layout-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null

# Which frame holds a split? `frames` prints one line per frame with the pane count, so this does not guess.
$panedFrame = $null
$kindsLine = $null
foreach ($line in (& $capture frames --session $session 2>$null)) {
    if ($line -match '^\s*frame\s+(\d+)\s+.*?(\d+)\s+panes') {
        $panedFrame = [int]$Matches[1]
        $kindsLine = $line.Trim()
        break
    }
}
if ($null -eq $panedFrame) {
    Write-Host ("{0} holds no frame with two panes, so a layout check has nothing to examine" -f $Session)
    exit 1
}
Write-Host ("  rendering {0}, frame {1}: {2}" -f $Session, $panedFrame, $kindsLine)

$document = Join-Path $work "paned.html"
& $capture render --session $session --seq $panedFrame --out $document 2>&1 | Out-Null
if (-not (Test-Path $document)) {
    Write-Host "the paned frame produced no document"
    exit 1
}

# UTF-8: read as the console default, a box character arrives mangled and the document looks empty.
$html = Get-Content -LiteralPath $document -Raw -Encoding UTF8
$problems = @()

if ($html -notmatch '<div class="panes"') {
    $problems += "the document has no pane container at all"
}
if ($html -notmatch 'data-layout="row"') {
    $problems += "the pane container does not declare a row layout"
}
$wrappers = ([regex]::Matches($html, '<div class="pane">')).Count
if ($wrappers -lt 2) {
    $problems += ("only {0} pane div(s) were drawn" -f $wrappers)
}

# The stylesheet, read as **one rule**: the first version of this searched the whole document and was satisfied by a
# `display: grid` in one rule and a `grid-template-columns` in another.
$rules = [regex]::Matches($html, '(?s)\.panes\s*\{([^}]*)\}')
if ($rules.Count -eq 0) {
    $problems += "no stylesheet rule for the pane container was written"
} else {
    $laysOutInRows = @($rules | Where-Object {
            $body = $_.Groups[1].Value
            ($body -match 'display:\s*(grid|flex)') -and ($body -match 'grid-template-columns:\s*[^;}]*\s[^;}]*')
        }).Count -gt 0
    if (-not $laysOutInRows) {
        $problems += "the pane container's stylesheet does not lay two tracks out in a row"
    }
}

# And the other direction: a frame with no split must not carry a pane container, or the marker means nothing.
$singleFrame = $null
foreach ($line in (& $capture frames --session $session 2>$null)) {
    if ($line -match '^\s*frame\s+(\d+)\s+.*?one pane') {
        $singleFrame = [int]$Matches[1]
        break
    }
}
if ($null -ne $singleFrame) {
    $single = Join-Path $work "single.html"
    & $capture render --session $session --seq $singleFrame --out $single 2>&1 | Out-Null
    if (Test-Path $single) {
        $singleHtml = Get-Content -LiteralPath $single -Raw -Encoding UTF8
        if ($singleHtml -match '<div class="panes"') {
            $problems += ("frame {0} holds no split and was drawn with one" -f $singleFrame)
        }
    }
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue

if ($problems.Count -gt 0) {
    $problems | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a paned page was not drawn as panes side by side"
    exit 1
}

Write-Host ("a paned page is drawn side by side: {0} pane div(s), a two-track row, and no container where there is no split" -f $wrappers)
exit 0
