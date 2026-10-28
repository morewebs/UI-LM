# Reports the screens this project cannot yet read perfectly.
#
# These are real recordings, kept outside the evaluation sets because they fail. Measuring them is
# the point: a known gap that nothing looks at becomes an unknown gap within a month, and the whole
# discipline here is that claims and evidence are the same document.
#
#   powershell -File scripts/check-hard.ps1
#
# Exit code 0 always — these are not gates, they are a scoreboard. What must not happen is that one
# of them silently gets *worse*, so each is compared against the figure recorded in
# docs/reports/2026-10-28-model.md and a regression is reported loudly.

param(
    [string]$Model = "models/page.json"
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }
$model = if ([System.IO.Path]::IsPathRooted($Model)) { $Model } else { Join-Path $repo $Model }
$cases = Join-Path $repo "cases\hard"

if (-not (Test-Path $cases)) {
    Write-Host "no hard cases on this machine (they are recorded locally and never committed)"
    exit 0
}

# The figures these screens reached when last measured. A drop here means something regressed.
$expected = @{
    "wsl-agent-in-pane"  = 100.0
    "wsl-agent-in-split" = 94.5
}

$regressed = 0
foreach ($case in (Get-ChildItem -LiteralPath $cases -Directory)) {
    $line = (& $capture score --session $case.FullName --model $model 2>&1 | Select-Object -First 1) -join ""
    $share = 0.0
    if ($line -match "model ([\d.]+)%") { $share = [double]$Matches[1] }
    $was = if ($expected.ContainsKey($case.Name)) { $expected[$case.Name] } else { $null }
    if ($null -eq $was) {
        Write-Host ("  {0,-22} {1,5:N1}%  (no expectation recorded)" -f $case.Name, $share)
    } elseif ($share -lt $was) {
        Write-Host ("  {0,-22} {1,5:N1}%  REGRESSED (was {2:N1}%)" -f $case.Name, $share, $was)
        $regressed++
    } else {
        Write-Host ("  {0,-22} {1,5:N1}%  (was {2:N1}%)" -f $case.Name, $share, $was)
    }
}

if ($regressed -gt 0) {
    Write-Host ""
    Write-Host "a known-hard screen got worse; that is a regression, not a gap"
    exit 1
}
Write-Host "known-hard screens are no worse than last measured"
