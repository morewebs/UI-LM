# The state page must name the release it is describing.
#
# A document that says "the download works" and names a version two releases old is worse than silent: it
# tells a reader what to download and is wrong. This compares the version in that row with the newest tag.

param()

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$page = Join-Path $repo "docs\reports\2026-10-28-state-of-the-project.md"
if (-not (Test-Path $page)) { Write-Host "no state page, so nothing was compared"; exit 1 }

$tags = @(& git -C $repo tag --list "v*" | ForEach-Object { $_.Trim() } | Where-Object { $_ -match '^v\d+\.\d+\.\d+$' })
if ($tags.Count -eq 0) { Write-Host "no release tags, so there is nothing to compare against"; exit 1 }
$latest = $tags | Sort-Object { [version]($_ -replace '^v', '') } | Select-Object -Last 1

$text = Get-Content -LiteralPath $page -Raw -Encoding UTF8
$cited = $null
foreach ($line in ($text -split "`n")) {
    if ($line -match 'the download works' -and $line -match 'v(\d+\.\d+\.\d+)') { $cited = "v" + $Matches[1] }
}

Write-Host ("  the state page names: {0}; the newest tag is: {1}" -f $(if ($cited) { $cited } else { "nothing" }), $latest)
if (-not $cited) {
    Write-Host "the state page no longer says which release it describes"
    exit 1
}
if ($cited -ne $latest) {
    Write-Host "the state page names a release that is not the newest one"
    exit 1
}
Write-Host "the state page names the release it is describing"

# The count the documents state must be the count the scripts folder holds. This was wrong three times in
# one day — off by one while counting the runner as a check, then off again while counting rows — and each
# version read as evidence. Counting it is cheaper than believing it.
$scripts = @(Get-ChildItem -LiteralPath (Join-Path $repo "scripts") -Filter "check-*.ps1" |
    Where-Object { $_.Name -ne "check-everything.ps1" }).Count
$fuzz = @(Get-ChildItem -Path (Join-Path $repo "crates\*\tests\fuzz_*.rs")).Count
$ledgerRows = @(Select-String -LiteralPath (Join-Path $repo "docs\checks.md") -Pattern '^\| `').Count

Write-Host ("  the folder holds {0} checks and {1} fuzz properties; the ledger lists {2} script(s)" -f $scripts, $fuzz, $ledgerRows)
if ($ledgerRows -ne $scripts) {
    Write-Host "the ledger does not list every check"
    exit 1
}

# The review names the newest release too, and it must not lag: it is the page a reader opens first.
$review = Join-Path $repo "docs\reports\2026-10-29-month-in-review.md"
if (Test-Path $review) {
    $reviewText = Get-Content -LiteralPath $review -Raw -Encoding UTF8
    $named = @([regex]::Matches($reviewText, 'v0\.\d+\.\d+') | ForEach-Object { $_.Value } | Sort-Object { [version]($_ -replace '^v', '') })
    if ($named.Count -gt 0 -and $named[-1] -ne $latest) {
        Write-Host ("  the review's newest version is {0}; the newest tag is {1}" -f $named[-1], $latest)
        Write-Host "the review names a release that is not the newest one"
        exit 1
    }
    Write-Host ("  the review's newest version is " + $named[-1])
}

$pages = @((Join-Path $repo "docs\reports\2026-10-28-state-of-the-project.md"), (Join-Path $repo "docs\reports\2026-10-29-month-in-review.md"))
$wrong = @()
foreach ($pagePath in $pages) {
    if (-not (Test-Path $pagePath)) { continue }
    $page = Get-Content -LiteralPath $pagePath -Raw -Encoding UTF8
    foreach ($match in [regex]::Matches($page, '(\d+)\s+scripts')) {
        if ([int]$match.Groups[1].Value -ne $scripts) {
            $wrong += ("{0}: says {1} scripts, the folder holds {2}" -f (Split-Path -Leaf $pagePath), $match.Groups[1].Value, $scripts)
        }
    }
}
if ($wrong.Count -gt 0) {
    $wrong | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a document counts the checks wrongly"
    exit 1
}
Write-Host "every document counts the checks the way the folder does"

# The corpus counts the documents state, counted here. Three of them had drifted — the batches, the
# training programs and the recordings — and each read as evidence while being wrong.
$recordings = 0
$frames = 0
foreach ($root in @("eval", "cases")) {
    $dir = Join-Path $repo $root
    if (-not (Test-Path $dir)) { continue }
    foreach ($session in (Get-ChildItem -LiteralPath $dir -Recurse -Directory | Where-Object { Test-Path (Join-Path $_.FullName "session.json") })) {
        $recordings++
        $file = Join-Path $session.FullName "frames.jsonl"
        if (Test-Path $file) { $frames += @(Get-Content -LiteralPath $file | Where-Object { $_.Trim() }).Count }
    }
}
Write-Host ("  the corpus holds {0} recording(s) and {1} frame(s)" -f $recordings, $frames)
if ($recordings -lt 1 -or $frames -lt 1) {
    Write-Host "no corpus on this machine, so the counts were not checked"
    exit 1
}

$corpusPage = Join-Path $repo "docs\reports\2026-10-28-state-of-the-project.md"
if (Test-Path $corpusPage) {
    $text = Get-Content -LiteralPath $corpusPage -Raw -Encoding UTF8
    $bad = @()
    foreach ($match in [regex]::Matches($text, '(\d+)\s+recordings')) {
        if ([int]$match.Groups[1].Value -ne $recordings) { $bad += ("the page says {0} recordings, the corpus holds {1}" -f $match.Groups[1].Value, $recordings) }
    }
    foreach ($match in [regex]::Matches($text, '(\d+)\s+frames audited')) {
        if ([int]$match.Groups[1].Value -ne $frames) { $bad += ("the page says {0} frames audited, the corpus holds {1}" -f $match.Groups[1].Value, $frames) }
    }
    if ($bad.Count -gt 0) {
        $bad | ForEach-Object { Write-Host ("  " + $_) }
        Write-Host "a document counts the corpus wrongly"
        exit 1
    }
}
Write-Host "the corpus counts in the documents match the corpus"

# A document that points at a file which is not there sends its reader nowhere. Only this repository's own
# `docs/` and `scripts/` paths are checked: the reports also cite paths from other projects and from
# machines that are gone, and those are history rather than instructions.
$sources = @(Get-ChildItem -LiteralPath (Join-Path $repo "docs") -Recurse -Filter "*.md") +
    @(Get-Item -LiteralPath (Join-Path $repo "README.md"), (Join-Path $repo "HANDOFF.md") -ErrorAction SilentlyContinue)
$dead = @()
foreach ($source in $sources) {
    $text = Get-Content -LiteralPath $source.FullName -Raw -Encoding UTF8
    foreach ($match in [regex]::Matches($text, '`((?:docs|scripts)/[^`]+\.(?:md|ps1))`')) {
        $target = Join-Path $repo ($match.Groups[1].Value -replace '/', '\')
        if (-not (Test-Path $target)) {
            $dead += ("{0} points at {1}, which does not exist" -f $source.Name, $match.Groups[1].Value)
        }
    }
}
if ($dead.Count -gt 0) {
    $dead | Sort-Object -Unique | Select-Object -First 5 | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a document points at a file that is not there"
    exit 1
}
Write-Host "every docs/ and scripts/ reference in the documents resolves"