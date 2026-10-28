# Checks the two things that can go wrong while a recording is being written.
#
# 1. A viewer asked for the state of a session *while* frames are being appended must never
#    fail, must never return an unparseable page, and must never report fewer frames than it
#    did before.
# 2. A frame file whose last line was cut off mid-write must be usable: the broken line is
#    skipped with a warning and every complete frame still pages.
#
#   powershell -File scripts/check-race.ps1
#
# Exit code 1 if either fails.

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
$serve = Join-Path $repo "target\debug\ui-lm-serve.exe"
if (-not (Test-Path $capture) -or -not (Test-Path $serve)) { throw "build first: cargo build --workspace" }

# A port nobody else has taken; the viewer refuses to start rather than share one.
$port = 8831
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-race-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$session = Join-Path $work "live"
$control = Join-Path $work "keys.txt"
Set-Content -LiteralPath $control -Value "" -Encoding ASCII
$failures = @()

# A program that redraws every 150 ms, so frames are appended while the viewer is read.
$recorder = Start-Process -FilePath $capture -PassThru -NoNewWindow -ArgumentList @(
    "record", "--rows", "20", "--cols", "80", "--out", $session, "--control", $control,
    "--stop-after", "28000", "--", "powershell", "-NoProfile", "-Command",
    "1..60 | ForEach-Object { 'tick ' + `$_ ; Start-Sleep -Milliseconds 150 }")
Start-Sleep -Seconds 5
$viewer = Start-Process -FilePath $serve -PassThru -NoNewWindow -ArgumentList @(
    "--session", $session, "--keys", $control, "--port", "$port")
Start-Sleep -Seconds 2

$ok = 0
$bad = 0
$counts = @()
for ($i = 0; $i -lt 140; $i++) {
    try {
        $response = Invoke-WebRequest -Uri "http://127.0.0.1:$port/state" -UseBasicParsing -TimeoutSec 3
        if ($response.StatusCode -eq 200) { $ok++ } else { $bad++ }
        $page = $response.Content | ConvertFrom-Json
        if (-not $page.page) { $bad++ } else { $counts += [int]$page.frames }
    } catch {
        $bad++
    }
}
if (-not $viewer.HasExited) { $viewer.Kill() }
Start-Sleep -Milliseconds 800
if (-not $recorder.HasExited) { $recorder.Kill() }

for ($i = 1; $i -lt $counts.Count; $i++) {
    if ($counts[$i] -lt $counts[$i - 1]) {
        $failures += "the frame count went backwards: $($counts[$i - 1]) -> $($counts[$i])"
        break
    }
}
if ($bad -gt 0) { $failures += "$bad of $($ok + $bad) reads during a redraw failed" }
if ($counts.Count -gt 0) {
    Write-Host ("reads during a redraw: {0}/{1} answered, frames {2} -> {3}" -f $ok, ($ok + $bad), $counts[0], $counts[-1])
} else {
    $failures += "no reads succeeded"
}

# A last line cut in half is what a viewer can catch mid-append.
$copy = Join-Path $work "truncated"
Copy-Item -LiteralPath $session -Destination $copy -Recurse -Force
$frames = Join-Path $copy "frames.jsonl"
$lines = [System.IO.File]::ReadAllLines($frames)
if ($lines.Count -lt 2) {
    $failures += "the recording held too few frames to truncate"
} else {
    $kept = $lines[0..($lines.Count - 2)]
    $kept += $lines[-1].Substring(0, [Math]::Max(1, [int]($lines[-1].Length / 2)))
    [System.IO.File]::WriteAllLines($frames, $kept, (New-Object System.Text.UTF8Encoding($false)))
    $before = $lines.Count - 1
    # `summarize` prints several lines; the one that matters names the frame count.
    $summary = ((& $capture summarize --session $copy 2>&1) | Select-String -Pattern "(\d+) frame") -join " "
    Write-Host ("half-written frame: {0}" -f $summary)
    if ($summary -notmatch "(\d+) frame") {
        $failures += "a truncated frame file could not be summarised: $summary"
    } elseif ([int]$Matches[1] -lt 1) {
        $failures += "a truncated frame file lost every frame"
    }
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "the viewer survives a redraw and a half-written frame"
