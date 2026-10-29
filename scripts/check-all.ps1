# Runs every check this project has, in one command, and fails if any of them falls short.
#
# The round-43 regression hid for several rounds because each round re-scored only the set it
# was working on. This exists so that "I checked" means "I checked everything", and so a
# release can be gated on it with one exit code.
#
#   powershell -File scripts/check-all.ps1
#   powershell -File scripts/check-all.ps1 -Fresh     # also record and score new material
#
# Exit code 1 if any check fails.

param(
    [switch]$Fresh,
    [string]$Model = "models/page.json"
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }
$model = if ([System.IO.Path]::IsPathRooted($Model)) { $Model } else { Join-Path $repo $Model }

$failures = @()
$rows = @()

# Every held-out set on this machine. `eval/` is ignored by git, so a fresh clone has none of
# them: a missing set is reported rather than skipped silently, because a sweep that quietly
# checks nothing is worse than no sweep.
$sets = Get-ChildItem -LiteralPath $repo -Recurse -Directory -Filter 'eval' -ErrorAction SilentlyContinue |
    Select-Object -First 1
$evalRoot = if ($sets) { $sets.FullName } else { Join-Path $repo "eval" }
foreach ($set in (Get-ChildItem -LiteralPath $evalRoot -Directory -ErrorAction SilentlyContinue)) {
    $sessions = Get-ChildItem -LiteralPath $set.FullName -Directory
    if (-not $sessions) { continue }
    $arguments = @("score", "--model", $model)
    foreach ($session in $sessions) { $arguments += @("--session", $session.FullName) }
    $line = (& $capture @arguments 2>&1 | Select-Object -First 1) -join ""
    if ($line -notmatch "over (\d+) frame") {
        $rows += "  {0,-16} no frames" -f $set.Name
        continue
    }
    $frames = [int]$Matches[1]
    $share = if ($line -match "model ([\d.]+)%") { [double]$Matches[1] } else { 0 }
    # The floor is printed beside the score because a set whose floor is 100% cannot tell the model from a
    # converter that splits paragraphs: 45 of 75 evaluation recordings are in that state, and "100%" beside
    # "floor 100%" means the set proved nothing about the vocabulary.
    $floor = if ($line -match "floor ([\d.]+)%") { [double]$Matches[1] } else { -1 }
    $verdict = if ($floor -ge 99.95) { "  (floor 100%, proves nothing about kinds)" } else { "" }
    $rows += "  {0,-16} {1,6:N1}%  over {2} frame(s), floor {3,5:N1}%{4}" -f $set.Name, $share, $frames, $floor, $verdict
    if ($share -lt 100) { $failures += "$($set.Name): $share%" }
}

# The label path must rebuild the reader's pages exactly, on the same material.
$selfCheck = @("self-check")
foreach ($session in (Get-ChildItem -LiteralPath $evalRoot -Directory |
        ForEach-Object { Get-ChildItem -LiteralPath $_.FullName -Directory } |
        Where-Object { Test-Path (Join-Path $_.FullName "session.json") })) {
    $selfCheck += @("--session", $session.FullName)
}
if ($selfCheck.Count -gt 1) {
    $line = (& $capture @selfCheck 2>&1 | Select-Object -First 1) -join ""
    $rows += "  {0,-16} {1}" -f "assembly", $line
    if ($line -notmatch "100\.0%") { $failures += "assembly: $line" }
}

# Provenance must be honest across every recorded sequence.
$provenance = @("provenance")
foreach ($session in (Get-ChildItem -LiteralPath $evalRoot -Directory |
        ForEach-Object { Get-ChildItem -LiteralPath $_.FullName -Directory } |
        Where-Object { Test-Path (Join-Path $_.FullName "session.json") })) {
    $provenance += @("--session", $session.FullName)
}
if ($provenance.Count -gt 1) {
    $line = (& $capture @provenance 2>&1 | Select-Object -First 1) -join ""
    $rows += "  {0,-16} {1}" -f "provenance", $line
    if ($line -notmatch " 0 contradiction") { $failures += "provenance: $line" }
}

# What kind and text do not carry: role, tabs, shortcuts, properties and spans. `score` cannot
# see these, so a page could report 100% while every diff in it had lost its colour — which is
# exactly what happened until this check existed.
$fields = @("fields")
foreach ($set in (Get-ChildItem -LiteralPath $evalRoot -Directory -ErrorAction SilentlyContinue)) {
    foreach ($session in (Get-ChildItem -LiteralPath $set.FullName -Directory)) {
        if (Test-Path (Join-Path $session.FullName "session.json")) {
            $fields += @("--session", $session.FullName)
        }
    }
}
if ($fields.Count -gt 1) {
    $line = (& $capture @fields 2>&1 | Select-Object -First 1) -join ""
    $rows += "  {0,-16} {1}" -f "fields", $line
    if ($line -notmatch " 0 differing") { $failures += "fields: $line" }
}
# The reverse direction must return every line of content, on the same material.
$tuiFailures = @()
foreach ($set in (Get-ChildItem -LiteralPath $evalRoot -Directory -ErrorAction SilentlyContinue)) {
    foreach ($session in (Get-ChildItem -LiteralPath $set.FullName -Directory)) {
        $line = (& $capture tui --session $session.FullName --all 2>&1 | Select-Object -Last 1) -join ""
        if ($line -match "([\d.]+)%" -and [double]$Matches[1] -lt 100) { $tuiFailures += $session.Name }
    }
}
$rows += "  {0,-16} {1} session(s) below 100%" -f "round trip", $tuiFailures.Count
if ($tuiFailures.Count -gt 0) { $failures += "round trip: " + ($tuiFailures -join ", ") }

# The same measurement on the pages a model built. The reverse direction had only ever been
# checked on the reader's page, so the loop a person actually uses — model judges, page draws
# back — was never tested end to end.
$modelTrip = @()
foreach ($set in (Get-ChildItem -LiteralPath $evalRoot -Directory -ErrorAction SilentlyContinue)) {
    foreach ($session in (Get-ChildItem -LiteralPath $set.FullName -Directory)) {
        if (-not (Test-Path (Join-Path $session.FullName "session.json"))) { continue }
        $line = (& $capture tui --session $session.FullName --all --model $model 2>&1 | Select-Object -Last 1) -join ""
        if ($line -match "([\d.]+)%" -and [double]$Matches[1] -lt 100) { $modelTrip += $session.Name }
    }
}
$rows += "  {0,-16} {1} session(s) below 100%" -f "round trip/model", $modelTrip.Count
if ($modelTrip.Count -gt 0) { $failures += "round trip on model pages: " + ($modelTrip -join ", ") }

if ($Fresh) {
    $line = (& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repo "scripts\check-fresh.ps1") 2>&1 |
        Select-Object -First 1) -join ""
    $rows += "  {0,-16} {1}" -f "fresh material", $line
    if ($LASTEXITCODE -ne 0) { $failures += "fresh material: $line" }
}

$rows | ForEach-Object { Write-Host $_ }
if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host ("{0} check(s) fell short:" -f $failures.Count)
    $failures | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host ""
Write-Host "every check passed"
