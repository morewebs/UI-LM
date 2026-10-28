# Fails if exporting a session gets much more expensive per page than it measured at.
#
# The renderer was measured linear to 1,510 pages: 4.4 ms and 3.9 KB per page at every size
# tried. That is a fact in a report until something enforces it, so this does.
#
#   powershell -File scripts/check-render-scale.ps1
#   powershell -File scripts/check-render-scale.ps1 -PageLimit 0.1   # prove it can fail
#
# Exit code 1 if a page costs more than the limit, or if the document is not well formed.

param(
    # Milliseconds per page allowed. The measured figure is 4.4; this leaves room for a slower
    # machine without leaving room for a change from linear to quadratic.
    [double]$PageLimit = 11.0,
    # Frames to export. Small enough to run often, large enough for a per-page average to mean
    # something.
    [int]$Frames = 60,
    # Peak resident memory allowed for the export. 150 pages measured 9 MB; this leaves room for
    # a different allocator without leaving room for an explosion.
    [double]$MemoryLimitMb = 250
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-renderscale-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$session = Join-Path $work "s"

# The program lives in a file rather than in a command string: quoting a `$_` through two
# layers of PowerShell produced a child that printed once and exited, and the check reported
# "only 1 page recorded" as if the recorder were at fault.
$program = Join-Path $work "draw.ps1"
Set-Content -LiteralPath $program -Encoding ASCII -Value @(
    "1..$Frames | ForEach-Object {",
    "    'line ' + `$_",
    "    Start-Sleep -Milliseconds 350",
    "}"
)
& $capture record --rows 24 --cols 100 --out $session --stop-after 45000 --cwd $work `
    -- powershell -NoProfile -File $program *> $null

$pages = (& $capture page --session $session --all 2>$null | Measure-Object).Count
if ($pages -lt 20) {
    Write-Host "only $pages page(s) recorded; too few to measure"
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}

# The render builds the whole document in memory before writing it, so its peak is watched as
# well as its duration: a straight line in time with a quiet explosion in memory would still be
# a regression.
$out = Join-Path $work "all.html"
$watch = [System.Diagnostics.Stopwatch]::StartNew()
$process = Start-Process -FilePath $capture -PassThru -NoNewWindow -ArgumentList @(
    "render", "--session", $session, "--all", "--out", $out)
$peak = 0
while (-not $process.HasExited) {
    try {
        $process.Refresh()
        if ($process.WorkingSet64 -gt $peak) { $peak = $process.WorkingSet64 }
    } catch { }
    Start-Sleep -Milliseconds 40
}
$watch.Stop()
$file = Get-Item $out
$html = Get-Content -LiteralPath $file.FullName -Raw
$perPage = $watch.Elapsed.TotalMilliseconds / $pages
$kbPerPage = ($file.Length / 1KB) / $pages

# Printed on every run: a check that reports only failures cannot be told from one that did not
# run.
Write-Host ("{0} page(s): {1} ms, {2:N0} KB, peak {3:N0} MB  ({4:N1} ms and {5:N1} KB per page; limits {6:N1} ms, {7:N0} MB)" -f `
        $pages, $watch.ElapsedMilliseconds, ($file.Length / 1KB), ($peak / 1MB), $perPage, $kbPerPage, $PageLimit, $MemoryLimitMb)

$failures = @()
if (-not ($html.StartsWith('<!doctype html>') -and $html.TrimEnd().EndsWith('</html>'))) {
    $failures += "the document was not well formed"
}
if (($peak / 1MB) -gt $MemoryLimitMb) {
    $failures += ("the export peaked at {0:N0} MB, more than the {1:N0} MB limit" -f ($peak / 1MB), $MemoryLimitMb)
}
if ($perPage -gt $PageLimit) {
    $failures += ("a page cost {0:N1} ms, more than the {1:N1} ms limit: exporting may no longer be linear" -f $perPage, $PageLimit)
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "exporting a session still costs about the same per page"
