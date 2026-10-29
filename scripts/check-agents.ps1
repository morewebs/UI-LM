# The second agent CLI: every recording of it must read perfectly, and none of it was ever trained on.
#
# Freebuff is the only application here that the model never saw. `check-hard` guards four hand-picked
# shapes of one application; this guards a whole other one, recorded through a different runtime on a
# different operating system, with its own glyphs for a tool row (`•`), its own reasoning heading
# (`• Thinking`) and its own diff rendering.
#
# The material is local, like the hard cases: captures are research data and stay out of the repository.

param()

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
$model = Join-Path $repo "models\page.json"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }
if (-not (Test-Path $model)) { Write-Host "no model, so nothing was scored"; exit 1 }

$root = Join-Path $repo "eval\freebuff"
if (-not (Test-Path $root)) {
    Write-Host "no second-agent recordings on this machine, so nothing was exercised"
    exit 1
}
$sessions = @(Get-ChildItem -LiteralPath $root -Recurse -Directory |
    Where-Object { Test-Path (Join-Path $_.FullName "session.json") })
if ($sessions.Count -eq 0) {
    Write-Host "the second-agent folder holds no recordings, so nothing was exercised"
    exit 1
}

$failures = @()
$frames = 0
$kinds = @{}
foreach ($session in $sessions) {
    $strong = (& $capture score --session $session.FullName --model $model 2>&1 | Select-Object -First 1) -join ""
    if ($strong -notmatch "model 100\.0%") { $failures += ("{0}: {1}" -f $session.Name, $strong.Trim()) }
    $trip = (& $capture tui --session $session.FullName --all --model $model 2>&1 | Select-Object -Last 1) -join ""
    if ($trip -notmatch "\(100\.0%\)") { $failures += ("{0} draws back weakly: {1}" -f $session.Name, $trip.Trim()) }
    $audit = (& $capture audit --model $model -- $session.FullName 2>&1 | Select-Object -Last 1) -join ""
    if ($audit -notmatch "no violations") { $failures += ("{0} contradicts its own screen: {1}" -f $session.Name, $audit.Trim()) }
    # `score` already walks every page once and prints a line per kind, so counting kinds costs nothing
    # more. A second pass over the same frames was a quarter of this check's running time.
    $scoreLines = @(& $capture score --session $session.FullName --model $model 2>&1)
    $frames += @($scoreLines | Where-Object { $_ -match "over (\d+) frame" } | ForEach-Object { [int]($_.ToString() -replace '.*over (\d+) frame.*', '$1') } | Measure-Object -Sum).Sum
    foreach ($line in $scoreLines) {
        if ("$line" -match "^\s+(\w+)\s+(\d+)/(\d+) reproduced") { $kinds[$Matches[1]] = [int]$kinds[$Matches[1]] + [int]$Matches[3] }
    }
}

Write-Host ("  {0} recording(s), {1} frame(s): {2}" -f $sessions.Count, $frames, (($kinds.GetEnumerator() | Sort-Object Key | ForEach-Object { $_.Key + "=" + $_.Value }) -join ", "))
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "the second agent does not read perfectly"
    exit 1
}
Write-Host "a second agent CLI reads perfectly, and was never trained on"