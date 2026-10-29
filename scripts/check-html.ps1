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
$panes = 0
$missing = @()
$model = Join-Path $repo "models\page.json"
# Both paths draw into the browser: the reader's pages and the model's. Only the reader's were ever
# checked, so a model page that reached the document short of a component would have passed silently —
# the same shape of gap as the diff that only appeared when someone asked for it.
foreach ($flavour in @("reader", "model")) {
foreach ($session in $sessions) {
    $modelArgs = if ($flavour -eq "model") { @("--model", $model) } else { @() }
    $screen = @(& $capture @(@("tui", "--session", $session.FullName, "--seq", "0") + $modelArgs) 2>$null)
    if ($screen.Count -eq 0) { continue }
    $out = Join-Path $work ("page-" + $flavour + ".html")
    & $capture @(@("render", "--session", $session.FullName, "--seq", "0", "--out", $out) + $modelArgs) 2>&1 | Out-Null
    if (-not (Test-Path $out)) {
        $missing += ("  " + $session.Name + " (" + $flavour + ") -> no document was written")
        continue
    }
    # The document is UTF-8. Read as the console default, a box character arrives mangled and every
    # line holding one looks missing — which is what an earlier version of this check reported.
    $raw = Get-Content -LiteralPath $out -Raw -Encoding UTF8
    $document = Visible $raw
    # Structure, not just words. A paned page drawn in one column still contains every word, which is
    # how the browser put an agent's meter above a shell's prompt for weeks without this check noticing.
    $page = (& $capture @(@("page", "--session", $session.FullName, "--seq", "0") + $modelArgs) 2>$null) -join ""
    if ($page -match '"panes"\s*:\s*\[\s*\[') {
        $wrappers = ([regex]::Matches($raw, '<div class="pane">')).Count
        if (-not ($raw -match '<div class="panes">')) {
            $missing += ("  " + $session.Name + " (" + $flavour + ") -> a paned page was drawn without a pane wrapper")
        } elseif ($wrappers -lt 2) {
            $missing += ("  " + $session.Name + " (" + $flavour + ") -> only " + $wrappers + " pane div(s) were drawn")
        }
        $panes++
    }
    foreach ($line in $screen) {
        # A document is markup, not a line printer: a tool call is three spans (`Read`,
        # `inventory.csv`, `+2 -2`) and the glyph in front of it is decoration. So the check asks
        # whether every *word* of every line reached the document, which is the claim that survives
        # both a fair rendering and a dropped component.
        foreach ($word in ($line -split '[^\w\.\-/]+')) {
            if ($word.Length -lt 4) { continue }
            $lines++
            if (-not $document.Contains($word)) {
                $missing += ("  " + $session.Name + " (" + $flavour + ") -> missing word: " + $word)
            }
        }
    }
}
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
Write-Host ("recordings checked: {0} on both paths, words checked: {1}, paned pages: {2}" -f $sessions.Count, $lines, $panes)
if ($missing.Count -gt 0) {
    $missing | Select-Object -First 10 | ForEach-Object { Write-Host $_ }
    Write-Host ("{0} line(s) never reached the document" -f $missing.Count)
    exit 1
}
Write-Host "every line the page holds reached the document"

# A screen can contain markup — an error message quoting a tag, a log line with an ampersand — and the
# document must hold it as text. This records such a screen and requires the payload to arrive escaped,
# because the alternative is a document that runs whatever the terminal printed.
$work2 = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-escape-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work2 | Out-Null
$payload = "<script>alert(1)</script>`nA & B <tag> C`n"
$encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($payload))
& wsl.exe -e sh -c "mkdir -p /tmp/ui-lm; echo $encoded | base64 -d > /tmp/ui-lm/escape.txt" 2>&1 | Out-Null
$session2 = Join-Path $work2 "s"
& $capture record --rows 12 --cols 70 --out $session2 --stop-after 8000 -- wsl.exe -e sh -c "cat /tmp/ui-lm/escape.txt; sleep 2" *> $null
$document2 = Join-Path $work2 "p.html"
& $capture render --session $session2 --seq 0 --out $document2 2>&1 | Out-Null

$failures = @()
if (Test-Path $document2) {
    $body = Get-Content -LiteralPath $document2 -Raw -Encoding UTF8
    if ($body -match "<script>alert") { $failures += "a screen's markup reached the document unescaped" }
    if ($body -notmatch "&lt;script&gt;") { $failures += "the markup was not shown as text either" }
    if ($body -notmatch "A &amp; B") { $failures += "an ampersand was not escaped" }
} else {
    $failures += "the escape check produced no document"
}
Remove-Item -LiteralPath $work2 -Recurse -Force -ErrorAction SilentlyContinue

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "the browser document is not safe against its own material"
    exit 1
}
Write-Host "markup on a screen arrives in the document as text, escaped"

# Frame 0 is rarely paned: a split appears once the application has drawn one. The two hard recordings
# have panes from frame 19, so a second pass renders one of those frames and requires the document to
# draw the panes as panes — the assertion above only runs when `page` says the frame is split.
$panedPages = 0
# The block below decides its own exit: appending it after the summary meant its findings were
# collected into $missing and then never looked at, which is why the assertion never fired.
$panedFailures = @()
New-Item -ItemType Directory -Force -Path $work | Out-Null
$hard = Join-Path $repo "cases\hard"
if (Test-Path $hard) {
    foreach ($session in (Get-ChildItem -LiteralPath $hard -Recurse -Directory | Where-Object { Test-Path (Join-Path $_.FullName "session.json") })) {
        foreach ($flavour in @("reader", "model")) {
            $modelArgs = if ($flavour -eq "model") { @("--model", $model) } else { @() }
            $document2 = Join-Path $work ("paned-" + $flavour + ".html")
            & $capture @(@("render", "--session", $session.FullName, "--seq", "20", "--out", $document2) + $modelArgs) 2>&1 | Out-Null
            if (-not (Test-Path $document2)) { continue }
            $page2 = (& $capture @(@("page", "--session", $session.FullName, "--seq", "20") + $modelArgs) 2>$null) -join ""
            if ($page2 -notmatch '"panes"\s*:\s*\[\s*\[') { continue }
            $raw2 = Get-Content -LiteralPath $document2 -Raw -Encoding UTF8
            $panedPages++
            $divs = ([regex]::Matches($raw2, '<div class="pane">')).Count
            if (-not ($raw2 -match '<div class="panes">')) {
                $panedFailures += ("  " + $session.Name + " (" + $flavour + ", frame 20) -> no pane wrapper")
            } elseif ($divs -lt 2) {
                $panedFailures += ("  " + $session.Name + " (" + $flavour + ", frame 20) -> " + $divs + " pane div(s)")
            }
        }
    }
}
Write-Host ("paned documents checked: {0}" -f $panedPages)
if ($panedFailures.Count -gt 0) {
    $panedFailures | ForEach-Object { Write-Host $_ }
    Write-Host "a paned page was not drawn as panes"
    exit 1
}