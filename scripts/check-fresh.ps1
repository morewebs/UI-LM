# Scores the model on screens recorded after it was trained.
#
# This is the only kind of test that can find a defect. Every other evaluation set in this
# project was built from screens that already exist, and a set everybody passes has stopped
# working: the batch recorded in round 40 found a missing tool verb within minutes, after
# twelve rounds of 100% on material that could not fail.
#
#   powershell -File scripts/check-fresh.ps1              # record a batch and score it
#   powershell -File scripts/check-fresh.ps1 -Keep        # keep the recordings
#
# Exit code is 1 when anything falls short, so it can gate a release.

param(
    [switch]$Keep,
    [string]$Model = "models/page.json"
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }
$model = if ([System.IO.Path]::IsPathRooted($Model)) { $Model } else { Join-Path $repo $Model }

# `-Keep` is always passed: without it the batch is cleaned up before this script can score it,
# and a check that never sees its own material would report nothing instead of failing.
$arguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $repo "scripts\build-corpus.ps1"), "-OutDir", "corpus/fresh", "-Keep")
$log = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-fresh-" + [guid]::NewGuid().ToString("N") + ".log")
# A batch normally takes four minutes. One took fifty-four, because this is the only check that starts
# a program and waits for it, and a session that never settles looks exactly like a slow one. The
# recorder bounds each scenario; nothing bounded the batch, so a hang was indistinguishable from
# progress. Twenty minutes is four times the usual: long enough for a slow machine, short enough to say
# something is wrong instead of leaving a run sitting there.
$limit = 1200
$watch = [System.Diagnostics.Stopwatch]::StartNew()
& powershell.exe @arguments *> $log
$watch.Stop()
$recorded = $watch.Elapsed.TotalSeconds
if ($recorded -gt $limit) {
    Write-Host ("recording the batch took {0:N0} s, beyond the {1} s limit" -f $recorded, $limit)
    Write-Host "a scenario that starts a program can hang, and this check would rather say so than wait"
    Write-Host ("the log is here: " + $log)
    exit 1
}

# The batches are kept in a temporary directory and named in the log; a batch that cannot be
# found is a failed check, not a pass.
$sessions = $null
$line = (Get-Content -LiteralPath $log | Select-String -Pattern 'recordings kept at' | Select-Object -Last 1)
if ($line) {
    $path = ($line.Line -replace '^.*recordings kept at ', '').Trim()
    if (Test-Path $path) { $sessions = $path }
}
if (-not $sessions) { Write-Host "no recordings were kept; the check did not run"; exit 1 }

$score = @("score", "--model", $model)
foreach ($session in (Get-ChildItem -LiteralPath $sessions -Directory)) { $score += @("--session", $session.FullName) }
$result = & $capture @score
$result | ForEach-Object { Write-Host $_ }

$model_line = $result | Select-String -Pattern 'model ([\d.]+)%' | Select-Object -First 1
if (-not $model_line) { Write-Host "the score said nothing"; exit 1 }
$share = [double]([regex]::Match($model_line.Line, 'model ([\d.]+)%').Groups[1].Value)
if ($share -lt 100) {
    Write-Host ""
    Write-Host "fresh material fell short: $share% - this is a defect, not a result"
    # Say *where*. A bare percentage sends the next person hunting through a batch of recordings
    # with a command line, which is what happened the last three times this check failed.
    Write-Host ""
    Write-Host "sessions below 100%, with what they missed:"
    foreach ($session in (Get-ChildItem -LiteralPath $sessions -Directory)) {
        $one = & $capture score --session $session.FullName --model $model --misses 2 2>&1
        $head = ($one | Select-Object -First 1) -join ""
        if ($head -match "model 100\.0%") { continue }
        Write-Host ("  {0}: {1}" -f $session.Name, ($head -replace ".*model ", "model "))
        $one | Where-Object { "$_" -match "^\s+(Block|Meter|ToolCall|Thought|Diff|Frame|Table)\s+\d+/" } |
            ForEach-Object { Write-Host ("    " + "$_".Trim()) }
        $one | Where-Object { "$_" -match "^  missed " } |
            Select-Object -First 2 |
            ForEach-Object { Write-Host ("    " + "$_".Trim().Substring(0, [Math]::Min(90, "$_".Trim().Length))) }
    }
    Write-Host ""
    Write-Host "the batch is kept at $sessions"
    exit 1
}
if (-not $Keep) { Remove-Item -LiteralPath $sessions -Recurse -Force -ErrorAction SilentlyContinue }
Write-Host "fresh material matched the model exactly"
