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