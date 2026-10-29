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
$base = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes("one`ntwo`nthree`n"))
$changed = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes("one`nTWO`nthree`nfour`n"))
& wsl.exe -e sh -c "mkdir -p /tmp/ui-lm/gdiff && cd /tmp/ui-lm/gdiff && git init -q . 2>/dev/null; echo $base | base64 -d > f.txt; git add f.txt; git -c user.email=a@b -c user.name=a commit -qm base 2>/dev/null; echo $changed | base64 -d > f.txt" 2>&1 | Out-Null

# The agent is asked to run `git diff` and mostly does. Once in a while it answers from context without
# running anything — which is not a defect in this project, but it does leave the check measuring nothing.
# So the scenario is attempted up to three times, and a run with no diff is retried rather than believed.
$attempts = 3
$keys = Join-Path $work "k.keys"
Set-Content -LiteralPath $keys -Value @('8000 y\r', '6000 \r', '8000 run git diff and show me the output\r', '25000 \r') -Encoding ASCII
$session = Join-Path $work "s"
# Up to three attempts: a run that produces no diff measured nothing, and is retried rather than believed.
$session = Join-Path $work "s"
$diffs = 0
$attempt = 0
while ($attempt -lt $attempts -and $diffs -lt 1) {
    $attempt++
    if (Test-Path $session) { Remove-Item -LiteralPath $session -Recurse -Force }
    & $capture record --rows 30 --cols 110 --out $session --stop-after 50000 --script $keys -- wsl.exe -e tmux new-session -A -s gdiffcheck "cd /tmp/ui-lm/gdiff && /mnt/c/Users/moreweb/.grok/bin/grok.exe" *> $null
    & wsl.exe -e sh -c 'tmux kill-server 2>/dev/null; true' 2>&1 | Out-Null
    if (-not (Test-Path (Join-Path $session "session.json"))) { continue }
    foreach ($line in (& $capture page --session $session --all 2>$null)) {
        if ("$line" -match '"component":"Diff"') { $diffs++ }
    }
    if ($diffs -lt 1) { Write-Host ("  attempt {0}: the agent did not show a diff, retrying" -f $attempt) }
}
if ($attempt -gt 1) { Write-Host ("  a diff appeared on attempt {0}" -f $attempt) }
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

if ($diffs -lt 1) { $failures += "no diff appeared, so the check measured nothing" }
if ($score -notmatch "model 100\.0%") { $failures += "the model missed something on a fresh diff: $score" }
if ($score -notmatch "Diff\s+(\d+)/(\d+) reproduced" -or $Matches[1] -ne $Matches[2]) {
    $failures += "the diff was not reproduced exactly: $score"
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a fresh diff was not read exactly"
    exit 1
}
Write-Host "a diff recorded after training was read exactly"