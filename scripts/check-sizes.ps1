# Records the same program at the smallest and largest screens this rig is likely to meet.
#
# Everything else is measured at 24x80. A page that works at one size and not another is a real
# failure mode — spans that run past the end of a line, panes that assume width — and the round trip
# is the measurement that would catch it.
#
# It also refuses to call a perfect score a pass on an empty screen: at least eight lines of content
# must come back, or the check says so instead of reporting "100% of nothing".
#
#   powershell -File scripts/check-sizes.ps1

param()

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-sizes-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null

$failures = @()
foreach ($size in @(@(10, 20), @(60, 200))) {
    $rows = $size[0]
    $cols = $size[1]
    $dir = Join-Path $work ("s{0}x{1}" -f $rows, $cols)
    & $capture record --rows $rows --cols $cols --out $dir --stop-after 9000 -- wsl.exe -e sh -c "seq 1 30; sleep 2" *> $null
    if (-not (Test-Path (Join-Path $dir "session.json"))) {
        $failures += ("{0}x{1}: the recording failed" -f $rows, $cols)
        continue
    }
    foreach ($flavour in @(@("reader", @()), @("model", @("--model", (Join-Path $repo "models\page.json"))))) {
        $arguments = @("tui", "--session", $dir, "--all") + $flavour[1]
        $line = (& $capture @arguments 2>&1 | Select-Object -Last 1) -join ""
        $share = 0.0
        $lines = 0
        if ($line -match "([\d.]+)%") { $share = [double]$Matches[1] }
        if ($line -match "(\d+)/(\d+) line") { $lines = [int]$Matches[1] }
        Write-Host ("  {0,-9} {1,-6} {2}" -f ("{0}x{1}" -f $rows, $cols), $flavour[0], $line.Trim())
        if ($share -lt 100) { $failures += ("{0}x{1} {2}: {3}" -f $rows, $cols, $flavour[0], $line.Trim()) }
        if ($lines -lt 8) { $failures += ("{0}x{1} {2}: only {3} line(s) of content, so nothing was really drawn" -f $rows, $cols, $flavour[0], $lines) }
    }
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a screen size broke the round trip"
    exit 1
}
Write-Host "both extremes round trip every line, on both paths"