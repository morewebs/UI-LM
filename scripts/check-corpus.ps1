# Reads every recording on this machine and fails if any of them breaks the reader.
#
# This is not a score check — `check-all` measures correctness on curated sets. This asks a blunter
# question: given everything ever recorded here, does the pipeline read all of it without crashing or
# coming back empty? A screen with a surprising shape should produce an odd page, not an error.
#
# The filter matters more than the loop. An earlier version of this sweep reported six failures that
# were all page *text* containing the word "error" — a `ReferenceError`, a PowerShell
# `CommandNotFoundException`, a fixture holding a Rust panic message. A check that cannot tell a
# symptom from the content it reads produces confident nonsense, so this one looks only at:
#
#   * the process exit code,
#   * anything written to standard error,
#   * a session that yields no page at all.
#
#   powershell -File scripts/check-corpus.ps1
#
# Exit code 1 if any recording fails to produce a page.

param(
    [string[]]$Roots = @("eval", "cases")
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$sessions = @()
foreach ($root in $Roots) {
    # A rooted path is used as given: Join-Path would mangle an absolute root into a child of the repo
    $full = if ([System.IO.Path]::IsPathRooted($root)) { $root } else { Join-Path $repo $root }
    if (-not (Test-Path $full)) { continue }
    $sessions += Get-ChildItem -LiteralPath $full -Recurse -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName "session.json") }
}

if ($sessions.Count -eq 0) {
    Write-Host "no recordings found, so nothing was exercised"
    exit 1
}

$failed = @()
$frames = 0
$model = Join-Path $repo "models\page.json"
# Both builders read every recording here. Only the reader's was checked, so a model path that fell over
# on one odd session — an empty screen, a shape it had never seen — would have gone unnoticed while
# every other check stayed green.
foreach ($flavour in @("reader", "model")) {
    $modelArgs = if ($flavour -eq "model") { @("--model", $model) } else { @() }
    foreach ($session in $sessions) {
        $out = & $capture @(@("page", "--session", $session.FullName, "--all") + $modelArgs) 2>&1
        $code = $LASTEXITCODE
        $pages = @($out | Where-Object { "$_".TrimStart().StartsWith("{") }).Count
        $stderr = @($out | Where-Object { "$_" -match "^ui-lm-capture:" })
        $frames += $pages

        if ($code -ne 0) {
            $failed += ("  " + $session.Name + " (" + $flavour + ") -> exit " + $code)
        } elseif ($stderr.Count -gt 0) {
            $failed += ("  " + $session.Name + " (" + $flavour + ") -> " + $stderr[0])
        } elseif ($pages -eq 0) {
            $failed += ("  " + $session.Name + " (" + $flavour + ") -> no page at all")
        }
    }
}

Write-Host ("recordings: {0} on both paths, pages read: {1}" -f $sessions.Count, $frames)
if ($failed.Count -gt 0) {
    $failed | ForEach-Object { Write-Host $_ }
    Write-Host ("{0} recording(s) broke the reader" -f $failed.Count)
    exit 1
}
Write-Host "every recording read without an error"

# A recording can be read perfectly and still be worthless: a capture of a program that failed holds almost
# nothing, and the reader will describe those few rows without complaint. The session now records how its
# program ended, so this asks the only question a reader cannot: did the program work?
$withStatus = 0
$broken = @()
foreach ($session in $sessions) {
    $record = Get-Content -LiteralPath (Join-Path $session.FullName "session.json") -Raw | ConvertFrom-Json
    if ($null -eq $record.exit) { continue }
    $withStatus++
    $frames = @(Get-Content -LiteralPath (Join-Path $session.FullName "frames.jsonl") | Where-Object { $_.Trim() }).Count
    if ($record.exit -ne 0 -and $frames -le 1) {
        $broken += ("{0}: the program exited {1} and drew {2} frame(s)" -f $session.Name, $record.exit, $frames)
    }
}
if ($withStatus -eq 0) {
    Write-Host "no recording carries an exit status, so nothing was judged about how its program ended"
    exit 1
}
if ($broken.Count -gt 0) {
    $broken | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a recording exists whose program failed and which holds almost nothing"
    exit 1
}
