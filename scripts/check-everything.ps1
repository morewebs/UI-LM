# Runs every check this project has, and prints one table.
#
# The checks are separate scripts because they answer separate questions, and merging them into one
# file would hide that. But "is this tree sound?" is a single question, and answering it by running
# eleven scripts one at a time is how a person stops running them.
#
#   powershell -File scripts/check-everything.ps1           # everything, about seven minutes
#   powershell -File scripts/check-everything.ps1 -Quick    # skip the two slowest, about a minute
#   powershell -File scripts/check-everything.ps1 -Tag v0.8.0   # also check that release's download
#
# Exit code 1 if any check fails. Every check runs even after one fails, because a table of one
# failure and ten passes says more than a table that stopped at the first line.

param(
    [switch]$Quick,
    [string]$Tag = ""
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

if (-not (Test-Path "target\debug\ui-lm-capture.exe")) {
    Write-Host "build first: cargo build --workspace"
    exit 1
}

# Order matters only for reading: cheap and broad first, the slow recordings last.
$all = @(
    @{ name = "gate";        script = "gate.ps1";        slow = $false },
    @{ name = "all";         script = "check-all.ps1";   slow = $false },
@{ name = "corpus";      script = "check-corpus.ps1"; slow = $false },
    @{ name = "audit";       script = "check-audit.ps1"; slow = $false },
    @{ name = "html";        script = "check-html.ps1";  slow = $false },
    @{ name = "hard";        script = "check-hard.ps1";  slow = $false },
@{ name = "validate";    script = "check-validate.ps1"; slow = $false },
    @{ name = "sizes";       script = "check-sizes.ps1"; slow = $false },
    @{ name = "race";        script = "check-race.ps1";  slow = $true },
    @{ name = "interaction"; script = "check-interaction.ps1"; slow = $true },
    @{ name = "live-page";   script = "check-live-page.ps1"; slow = $true },
    @{ name = "render-scale"; script = "check-render-scale.ps1"; slow = $true },
    @{ name = "fresh";       script = "check-fresh.ps1"; slow = $true }
)

$selected = if ($Quick) { $all | Where-Object { -not $_.slow } } else { $all }
$results = @()
foreach ($check in $selected) {
    $script = Join-Path $PSScriptRoot $check.script
    if (-not (Test-Path $script)) { continue }
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $out = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $script 2>&1
    $code = $LASTEXITCODE
    $watch.Stop()
    $last = ($out | Where-Object { "$_".Trim() } | Select-Object -Last 1) -join ""
    $results += [pscustomobject]@{
        check   = $check.name
        code    = $code
        seconds = [Math]::Round($watch.Elapsed.TotalSeconds, 1)
        line    = $last.Trim()
    }
    Write-Host ("  {0,-14} {1,-6} {2,6}s  {3}" -f $check.name,
        $(if ($code -eq 0) { "ok" } else { "FAILED" }), $results[-1].seconds, $results[-1].line)
}

if ($Tag -ne "") {
    $out = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "check-release.ps1") -Tag $Tag 2>&1
    $code = $LASTEXITCODE
    $results += [pscustomobject]@{ check = "release $Tag"; code = $code; seconds = 0; line = (($out | Where-Object { "$_".Trim() } | Select-Object -Last 1) -join "").Trim() }
    Write-Host ("  {0,-14} {1,-6}         {2}" -f "release", $(if ($code -eq 0) { "ok" } else { "FAILED" }), $results[-1].line)
}

$failed = @($results | Where-Object { $_.code -ne 0 })
$total = [Math]::Round(($results | Measure-Object -Property seconds -Sum).Sum, 1)
Write-Host ""
if ($failed.Count -gt 0) {
    Write-Host ("{0} of {1} check(s) failed in {2}s" -f $failed.Count, $results.Count, $total)
    $failed | ForEach-Object { Write-Host ("  " + $_.check + ": " + $_.line) }
    exit 1
}
Write-Host ("every check passed in {0}s" -f $total)
