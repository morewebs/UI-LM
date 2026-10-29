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