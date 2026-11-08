# Runs every check this project has, and prints one table.
#
# The checks are separate scripts because they answer separate questions, and merging them into one
# file would hide that. But "is this tree sound?" is a single question, and answering it by running
# eleven scripts one at a time is how a person stops running them.
#
#   powershell -File scripts/check-everything.ps1           # everything, about half an hour
#   powershell -File scripts/check-everything.ps1 -Quick    # skip the nine slowest, about fifteen minutes
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

# The checks need all three binaries, and nothing else here builds them. `cargo test` produces
# `ui-lm-capture.exe` for its integration tests, but `ui-lm-serve.exe` and `ui-lm-validate.exe` come only from
# `cargo build --workspace`. On 2026-11-08 an invalidated `target/` left the capture binary in place and five checks
# spent the run reporting "no binaries, so nothing was measured" - five false failures that named no defect. The runner
# produces what it measures, and stops if it cannot.
$buildStarted = Get-Date
& cargo build --workspace *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Host "cargo build --workspace failed, so nothing can be measured"
    exit 1
}
$buildSeconds = ((Get-Date) - $buildStarted).TotalSeconds
if ($buildSeconds -gt 5) {
    Write-Host ("  the workspace took {0:N0}s to build" -f $buildSeconds)
}

# The other half of the orphan fix (rounds 166-168). Assigning this process to a job whose members are killed when its
# last handle closes means an interrupted run takes its checks down with it, however it is interrupted. The sweep below
# stays as belt and braces, and is what still finds children from runs that predate this.
. (Join-Path $PSScriptRoot "job-object.ps1")
if (Enter-KillOnCloseJob) {
    Write-Host "  this run's checks are members of a job that dies with it"
}

# A killed run leaves its children behind - `powershell.exe` per check - and eleven of them were found burning five CPU-hours
# each, which made every later run look slow and four checks look flaky (round 166). Sweep stale children before starting, and
# say what was swept, because a suite that measures a busy machine measures the machine.
$stale = @(Get-Process powershell -ErrorAction SilentlyContinue | Where-Object {
    $_.StartTime -lt (Get-Date).AddMinutes(-30)
})
if ($stale.Count -gt 0) {
    $stale | ForEach-Object { try { $_.Kill() } catch { } }
    Write-Host ("swept {0} stale shell(s) from an earlier run" -f $stale.Count)
}
Get-Process ui-lm-serve, ui-lm-capture -ErrorAction SilentlyContinue | ForEach-Object { try { $_.Kill() } catch { } }

# Order matters only for reading: cheap and broad first, the slow recordings last.
$all = @(
    @{ name = "gate";        script = "gate.ps1";        slow = $false },
    @{ name = "all";         script = "check-all.ps1";   slow = $false },
@{ name = "corpus";      script = "check-corpus.ps1"; slow = $false },
    @{ name = "audit";       script = "check-audit.ps1"; slow = $false },
    @{ name = "html";        script = "check-html.ps1";  slow = $false },
    @{ name = "kinds";       script = "check-kinds.ps1"; slow = $false },
    @{ name = "html-to-tui"; script = "check-html-to-tui.ps1"; slow = $false },
    @{ name = "loop";      script = "check-loop.ps1"; slow = $true },
    @{ name = "layout";      script = "check-layout.ps1"; slow = $false },
    @{ name = "hard";        script = "check-hard.ps1";  slow = $false },
@{ name = "agents";      script = "check-agents.ps1"; slow = $true },
@{ name = "validate";    script = "check-validate.ps1"; slow = $false },
@{ name = "sizes";       script = "check-sizes.ps1"; slow = $false },
@{ name = "scrub";       script = "check-scrub.ps1"; slow = $false },
@{ name = "corrupt";     script = "check-corrupt.ps1"; slow = $false },
@{ name = "orphans";     script = "check-orphans.ps1"; slow = $false },
@{ name = "encoding";    script = "check-encoding.ps1"; slow = $false },
@{ name = "readme";      script = "check-readme.ps1"; slow = $false },
    @{ name = "docs";        script = "check-docs.ps1"; slow = $false },
    @{ name = "claims";      script = "check-claims.ps1"; slow = $false },
    @{ name = "enumerations"; script = "check-enumerations.ps1"; slow = $false },
    @{ name = "tables";      script = "check-tables.ps1"; slow = $false },
    @{ name = "schema";      script = "check-schema.ps1"; slow = $true },
    @{ name = "conformance"; script = "check-conformance.ps1"; slow = $false },
    @{ name = "race";        script = "check-race.ps1";  slow = $true },
    @{ name = "interaction"; script = "check-interaction.ps1"; slow = $true },
    @{ name = "live-page";   script = "check-live-page.ps1"; slow = $true },
    @{ name = "render-scale"; script = "check-render-scale.ps1"; slow = $true },
@{ name = "fresh";       script = "check-fresh.ps1"; slow = $true },
    @{ name = "diff-fresh";  script = "check-diff-fresh.ps1"; slow = $true }
)

$selected = if ($Quick) { $all | Where-Object { -not $_.slow } } else { $all }
$results = @()
# Each check runs as a tracked child, so that a run which is interrupted can take its children with it. `&` gives no PID and the
# orphans from killed runs were what made four checks look flaky and a 30-minute suite take an hour (rounds 166-168).
$running = @()
foreach ($check in $selected) {
    $script = Join-Path $PSScriptRoot $check.script
    if (-not (Test-Path $script)) { continue }
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $log = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-check-" + [guid]::NewGuid().ToString("N") + ".log")
    $child = Start-Process -FilePath "powershell.exe" -PassThru -NoNewWindow -Wait -ArgumentList @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $script) -RedirectStandardOutput $log
    $running += $child.Id
    $out = @(if (Test-Path $log) { Get-Content -LiteralPath $log -Encoding UTF8 })
    Remove-Item -LiteralPath $log -Force -ErrorAction SilentlyContinue
    $code = $child.ExitCode
    $first = ($out | Where-Object { "$_".Trim() } | Select-Object -Last 1) -join ""
    # Five checks have been red once and green the next time - race, loop, layout, validate, sizes - and every one of them
    # starts a process, opens a port or reads a file another check just wrote (round 136). A single flake must not read as a
    # regression, and a real failure must still read as red, so a failed check is run **once more** and both outcomes are
    # reported. This is a retry that is written down and printed, not a quiet re-run.
    $again = $false
    if ($code -ne 0) {
        $again = $true
        Start-Sleep -Seconds 5
        $log = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-check-" + [guid]::NewGuid().ToString("N") + ".log")
        $child = Start-Process -FilePath "powershell.exe" -PassThru -NoNewWindow -Wait -ArgumentList @(
            "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $script) -RedirectStandardOutput $log
        $running += $child.Id
        $out = @(if (Test-Path $log) { Get-Content -LiteralPath $log -Encoding UTF8 })
        Remove-Item -LiteralPath $log -Force -ErrorAction SilentlyContinue
        $code = $child.ExitCode
    }
    $watch.Stop()
    $last = (($out | Where-Object { "$_".Trim() } | Select-Object -Last 1) -join "")
    if ($again) {
        $verdict = if ($code -eq 0) { "flaked" } else { "FAILED" }
        $last = ("{0} (first run: {1})" -f $last, $first)
    } else {
        $verdict = "ok"
    }
    $results += [pscustomobject]@{
        check   = $check.name
        code    = $code
        seconds = [Math]::Round($watch.Elapsed.TotalSeconds, 1)
        line    = $last.Trim()
    }
    Write-Host ("  {0,-14} {1,-6} {2,6}s  {3}" -f $check.name, $verdict, $results[-1].seconds, $results[-1].line)
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
