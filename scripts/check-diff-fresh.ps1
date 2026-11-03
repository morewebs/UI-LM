# Records a screen holding a real diff, makes it after training, and requires the model to read it.
#
# `Diff` is the only kind the training application never draws unprompted: an edit appears as a tool
# line with counts (`+1/-1`), so a normal batch contains four kinds. Asking the agent to run `git diff`
# in a repository with a real change makes the output block hold a two-sided diff, and this check keeps
# that in the record rather than in a note.
#
#   powershell -File scripts/check-diff-fresh.ps1
#
# Exit code 1 if no diff appears, or if the model misses any component on the recording.

param()

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-diff-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null

# A repository with a real change, written through base64 so no shell can eat the newlines.
# A repository on this machine, not inside WSL: the agent that shows a diff today is the one that runs on Windows, and a
# `/tmp` fixture was wiped whenever the WSL VM idled out (rounds 57-60).
$fixture = Join-Path $work "gdiff"
New-Item -ItemType Directory -Force -Path $fixture | Out-Null
Set-Content -LiteralPath (Join-Path $fixture "f.txt") -Value "one`ntwo`nthree`n" -NoNewline -Encoding UTF8
& git -C $fixture init -q . 2>&1 | Out-Null
& git -C $fixture add -A 2>&1 | Out-Null
& git -C $fixture -c user.email=a@b -c user.name=a commit -qm base 2>&1 | Out-Null
Set-Content -LiteralPath (Join-Path $fixture "f.txt") -Value "one`nTWO`nthree`nfour`n" -NoNewline -Encoding UTF8

# The agent is asked to run `git diff` and mostly does. Once in a while it answers from context without
# running anything — which is not a defect in this project, but it does leave the check measuring nothing.
# So the scenario is attempted up to three times, and a run with no diff is retried rather than believed.
$keys = Join-Path $work "k.keys"
# The agent no longer asks the trust question this script was written for: sending `y` typed it into the prompt and the
# real request was never submitted. A blank line, then the request, then a blank line again, and a longer wait for the
# answer (round 57 - the suite had been red for two days on a keystroke that no longer matched the screen).
# Asking only for `git diff` leaves the agent free to answer from what it read. Asking it to **edit** and then show the
# diff made it reliable in the same minute: 18 diffs in one session against 12 from the earlier wording (round 65).
Set-Content -LiteralPath $keys -Value @('14000 ', '3000 edit f.txt to add a line, then run git diff and show me the output', '3000 \r', '55000 ') -Encoding ASCII
$session = Join-Path $work "s"
# The scenario is attempted up to five times: a live agent fails to show a diff about one run in
# three, so three attempts left this check red roughly half the time. Five leave it under 1%.
$attempts = 5
$diffs = 0
$attempt = 0
while ($attempt -lt $attempts -and $diffs -lt 1) {
    $attempt++
    if (Test-Path $session) { Remove-Item -LiteralPath $session -Recurse -Force }
    & $capture record --rows 30 --cols 110 --out $session --stop-after 60000 --script $keys -- cmd.exe /c "freebuff --cwd $fixture" *> $null
    & wsl.exe -e sh -c 'tmux kill-server 2>/dev/null; true' 2>&1 | Out-Null
    if (-not (Test-Path (Join-Path $session "session.json"))) { continue }
    foreach ($line in (& $capture page --session $session --all 2>$null)) {
        if ("$line" -match '"component":"Diff"') { $diffs++ }
    }
    if ($diffs -lt 1) { Write-Host ("  attempt {0}: the agent did not show a diff, retrying" -f $attempt) }
}
if ($diffs -ge 1 -and $attempt -gt 1) { Write-Host ("  a diff appeared on attempt {0}" -f $attempt) }
$failures = @()
if (-not (Test-Path (Join-Path $session "session.json"))) {
    Write-Host "the recording failed, so nothing was measured"
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}

$diffs = 0
foreach ($line in (& $capture page --session $session --all 2>$null)) {
    if ("$line" -match '"component":"Diff"') { $diffs++ }
}

$score = (& $capture score --session $session --model (Join-Path $repo "models\page.json") 2>&1 | Select-Object -First 6) -join " | "
Write-Host ("  diff components: {0}" -f $diffs)
Write-Host ("  {0}" -f $score)

# A live agent answers, then stops answering, then answers again (rounds 76-77). The axis this check measures is **a diff
# recorded after training is read exactly**, not "an agent drew it", so when the agent shows nothing the diff comes from a
# program that always draws one: `git diff` in a repository whose content was written after training.
if ($diffs -lt 1) {
    $writer = Join-Path $work "git.keys"
    Set-Content -LiteralPath $writer -Value @('4000 ') -Encoding ASCII
    & $capture record --rows 30 --cols 110 --out $session --stop-after 15000 --script $writer -- git -C $fixture diff *> $null
    $diffs = 0
    foreach ($line in (& $capture page --session $session --all 2>$null)) {
        if ("$line" -match '"component":"Diff"') { $diffs++ }
    }
    if ($diffs -ge 1) {
        $usedFallback = $true
        Write-Host ("  the agent showed nothing, so the diff came from git itself: {0} component(s)" -f $diffs)
        $score = (& $capture score --session $session --model (Join-Path $repo "models\page.json") 2>&1 | Select-Object -First 1)
        Write-Host ("  {0}" -f $score)
    }
}

if ($diffs -lt 1) { $failures += "no diff appeared, so the check measured nothing" }
if ($score -notmatch "model 100\.0%") { $failures += "the model missed something on a fresh diff: $score" }
# A one-frame git diff has no per-kind table in its score line; the model's 100% and the component count are the claim.
if (-not $usedFallback -and ($score -notmatch "Diff\s+(\d+)/(\d+) reproduced" -or $Matches[1] -ne $Matches[2])) {
    $failures += "the diff was not reproduced exactly: $score"
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    # "the agent showed no diff" and "the reader got the diff wrong" are different failures, and reporting the
    # first as the second is how a flaky agent looks like a broken reader. Round 90 found the full suite red for
    # this reason while the same check passed on its own a minute later.
    if ($failures.Count -eq 1 -and $failures[0] -match "no diff appeared") {
        Write-Host "the agent showed no diff in three attempts, so nothing was measured"
        exit 1
    }
    Write-Host "a fresh diff was not read exactly"
    exit 1
}
Write-Host "a diff recorded after training was read exactly"