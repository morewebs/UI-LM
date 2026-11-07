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
# The batch also records one screen for each kind a program draws without an agent - a diff, a word grid and a dialog -
# and a probe that does not draw its kind fails the check. A batch without that material cannot find a defect in those
# rules, and a batch that lost it silently is the failure this check exists to make impossible.
#
# Exit code is 1 when anything falls short, so it can gate a release.

param(
    [switch]$Keep,
    [string]$Model = "models/page.json"
)

. (Join-Path $PSScriptRoot "utf8.ps1")
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

# --- the kinds a batch must hold ---------------------------------------------------------------
# What a fresh batch can score is what it recorded. Without an agent CLI on PATH the standard scenarios
# draw only Block and Meter, so a rule broken by *new* output survives: check-hard's cases are fixed
# recordings. These probes record one screen for each kind a program draws without an agent, into the
# same batch, so the score runs on them. A probe that does not draw its kind fails - a batch that
# silently lost its material is the failure this check exists to make impossible.
$probeFailures = @()
$probeWork = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-probe-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $probeWork | Out-Null

function Add-Probe {
    param([string]$Name, [string]$Kind, [int]$Rows, [int]$Cols, [int]$Stop, [string[]]$Arguments)
    $out = Join-Path $sessions $Name
    if (Test-Path $out) { Remove-Item -LiteralPath $out -Recurse -Force }
    & $capture record --rows $Rows --cols $Cols --out $out --stop-after $Stop -- @Arguments *> $null
    if (-not (Test-Path (Join-Path $out "session.json"))) {
        $script:probeFailures += ("{0}: the recorder wrote nothing" -f $Name)
        return
    }
    $frame = (((& $capture frames --session $out 2>&1 | Select-Object -Last 1) -join " ") -replace "\s{2,}", " ").Trim()
    if ($frame -notmatch ($Kind + "=[0-9]")) {
        $script:probeFailures += ("{0}: the last frame holds no {1} - it says '{2}'" -f $Name, $Kind, $frame)
        return
    }
    $trip = (((& $capture tui --session $out --all 2>&1 | Select-Object -Last 1) -join "")).Trim()
    if ($trip -notmatch [regex]::Escape("(100.0%)")) {
        $script:probeFailures += ("{0}: the page draws back at '{1}', and every frame here must round trip" -f $Name, $trip)
    }
}

$git = Get-Command git -ErrorAction SilentlyContinue
if ($git) {
    $probeRepo = Join-Path $probeWork "repo"
    New-Item -ItemType Directory -Force -Path $probeRepo | Out-Null
    & git -C $probeRepo init -q 2>&1 | Out-Null
    & git -C $probeRepo config user.email probe@example.com
    & git -C $probeRepo config user.name probe
    Set-Content -LiteralPath (Join-Path $probeRepo "notes.txt") -Value "alpha`nbeta`n" -Encoding ASCII
    & git -C $probeRepo add -A 2>&1 | Out-Null
    & git -C $probeRepo commit -qm "init" 2>&1 | Out-Null
    Set-Content -LiteralPath (Join-Path $probeRepo "notes.txt") -Value "alpha`nBETA`n" -Encoding ASCII
    Add-Probe -Name "probe-diff" -Kind "Diff" -Rows 24 -Cols 90 -Stop 2500 -Arguments @("git", "-C", $probeRepo, "-c", "color.ui=always", "diff")
} else {
    Write-Host "  no git, so the Diff probe was skipped"
}

if (Get-Command wsl.exe -ErrorAction SilentlyContinue) {
    # A cold WSL start can outlast a short stop deadline and record a blank screen with no exit status;
    # warming the VM first makes the probe about the listing rather than about the machine.
    & wsl.exe -e true 2>$null | Out-Null
    Add-Probe -Name "probe-grid" -Kind "Table" -Rows 40 -Cols 120 -Stop 6000 -Arguments @("wsl.exe", "-e", "ls", "-C", "/usr/lib")
    if (((& wsl.exe -e sh -c "command -v whiptail" 2>$null) -join "").Trim()) {
        Add-Probe -Name "probe-dialog" -Kind "Frame" -Rows 24 -Cols 80 -Stop 4000 -Arguments @("wsl.exe", "-e", "sh", "-c", "whiptail --title 'Probe' --yesno 'A frame?' 12 50 2>/dev/null; sleep 1")
    } else {
        Write-Host "  no whiptail in wsl, so the Frame probe was skipped"
    }
} else {
    Write-Host "  no wsl.exe, so the Table and Frame probes were skipped"
}
Remove-Item -LiteralPath $probeWork -Recurse -Force -ErrorAction SilentlyContinue
if ($probeFailures.Count -gt 0) {
    $probeFailures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a probe did not draw the kind it exists for"
    Write-Host "the batch is kept at $sessions"
    exit 1
}

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
