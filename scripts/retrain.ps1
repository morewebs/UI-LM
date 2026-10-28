# Records, trains and evaluates in one run.
#
# Three recordings are needed for an honest result: two batches to train on and a third to
# be scored against, plus screens from programs that are not the training application. This
# script does all of it and prints one summary, so the numbers are produced by the same code
# every time rather than by whatever was done by hand.
#
#   powershell -File scripts/retrain.ps1
#
# It takes a while: every scenario waits for a real program to draw and settle.

param(
    [string]$Model = "models/page.json"
)

# A child program can print a warning on stderr — a console control event, a missing hook —
# and PowerShell 5.1 treats that as a terminating error. The script's own failures are read
# from exit codes instead, so a warning must not stop it.
$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
$sessionRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-retrain-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $sessionRoot | Out-Null

# Shapes the training application never draws: an edit report, which is what a diff
# component is made of. Without screens like these the component is invisible to the model.
$diffProgram = Join-Path $sessionRoot "diff.py"
@'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
line("Changes to config.txt")
line("")
line("  - retries = 3")
line("  + retries = 5")
line("")
line("  - timeout = 30")
line("  + timeout = 60")
line("")
line("2 lines changed")
time.sleep(3)
'@ | Set-Content -LiteralPath $diffProgram -Encoding UTF8

function Record-Diffs([string]$name) {
    $out = Join-Path $sessionRoot $name
    & $capture record --rows 20 --cols 70 --out $out --stop-after 4500 -- python $diffProgram *> $null
    return $out
}

# Two training batches.
$trainSessions = @()
foreach ($batch in 1..2) {
    $log = Join-Path $sessionRoot "corpus-$batch.txt"
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repo "scripts\build-corpus.ps1") -Keep -Out (Join-Path $sessionRoot "corpus$batch") *> $log
    # `-Out` is where the pairs are written; the recordings are kept in the script's own
    # scratch directory and it says where. Reading that rather than guessing is the
    # difference between training on the corpus and training on one screen.
    $kept = (Get-Content -LiteralPath $log | Select-String -Pattern 'recordings kept at (.+)$').Matches.Groups[1].Value.Trim()
    Get-ChildItem -LiteralPath $kept -Directory | ForEach-Object { $trainSessions += $_.FullName }
    Write-Host "training batch $batch recorded"
}
foreach ($name in @("diff-a", "diff-b")) {
    $trainSessions += Record-Diffs $name
}

# A diff is made of minus and plus lines, and the model has to see enough of them to be
# able to produce the component at all. Two screens gave four rows out of eight thousand and
# the model never learned it, so this records a dozen varied edit reports instead.
$edits = @(
    @("config.txt", "retries = 3", "retries = 5"),
    @("server.toml", "port = 8080", "port = 9090"),
    @("styles.css", "color: red", "color: navy"),
    @("notes.md", "draft", "final"),
    @("plan.py", "steps = 4", "steps = 6"),
    @("readme.md", "install", "set up"),
    @("data.csv", "12,7", "14,7"),
    @("route.js", "path: /old", "path: /new"),
    @("schema.sql", "INT", "BIGINT"),
    @("make.sh", "make test", "make check"),
    @("theme.json", '"dark"', '"darker"'),
    @("limit.conf", "max = 100", "max = 250")
)
$variant = 0
foreach ($edit in $edits) {
    $variant += 1
    $script = Join-Path $sessionRoot ("diff-$variant.py")
    $body = @"
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode('utf-8') + b'\r\n'); out.flush()
line('Changes to $($edit[0])')
line('')
line('  - $($edit[1])')
line('  + $($edit[2])')
line('')
line('1 line changed')
time.sleep(2)
"@
    Set-Content -LiteralPath $script -Value $body -Encoding UTF8
    $out = Join-Path $sessionRoot "edit-$variant"
    & $capture record --rows 16 --cols 70 --out $out --stop-after 3500 -- python $script *> $null
    if (Test-Path (Join-Path $out "session.json")) { $trainSessions += $out }
}
Write-Host "recorded $($edits.Count) edit screens"

# Training pairs.
$pairs = Join-Path $sessionRoot "train.jsonl"
$arguments = @("dataset", "--out", $pairs, "--scrub")
foreach ($session in $trainSessions) { $arguments += @("--session", $session) }
& $capture @arguments *> $null
Write-Host "exported $((Get-Content $pairs).Count) pair(s)"

# Train.
& python (Join-Path $repo "scripts\train_model.py") $pairs (Join-Path $repo $Model) | Write-Host

# Two evaluations: a fresh batch from the training application, and one from others.
$evalLog = Join-Path $sessionRoot "eval.txt"
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repo "scripts\build-corpus.ps1") -Keep -Out (Join-Path $sessionRoot "eval") *> $evalLog
$grokEval = @("score", "--model", $Model)
$evalKept = (Get-Content -LiteralPath $evalLog | Select-String -Pattern 'recordings kept at (.+)$').Matches.Groups[1].Value.Trim()
Get-ChildItem -LiteralPath $evalKept -Directory | ForEach-Object { $grokEval += @("--session", $_.FullName) }
Write-Host ""
Write-Host "=== unseen batch, training application ==="
& $capture @grokEval

& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repo "scripts\build-nongrok.ps1") -Out (Join-Path $sessionRoot "nongrok") | Out-Null
$otherEval = @("score", "--model", $Model)
Get-ChildItem -LiteralPath (Join-Path $sessionRoot "nongrok") -Directory | ForEach-Object { $otherEval += @("--session", $_.FullName) }
Write-Host "=== unseen batch, other applications ==="
& $capture @otherEval

Write-Host ""
Write-Host "recordings kept at $sessionRoot"
