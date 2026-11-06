# A killed suite must take its children with it.
#
# `check-everything` starts one powershell per check. When a run was interrupted, its children survived: eleven of them
# were found burning five CPU-hours each, and four checks looked flaky because they were measuring a busy machine
# (rounds 166-168). The runner now installs a job object with KILL_ON_JOB_CLOSE, so the operating system kills the
# children when the runner's handle closes - which happens however the runner dies.
#
# This check is the experiment, not the code path: it starts a runner, kills the runner, and asks whether the runner's
# child is still there. It runs the same experiment **without** the job object first, because a check that only ever
# sees a child die cannot tell a kill-on-close job from a runner that never started one.

param()

. (Join-Path $PSScriptRoot "utf8.ps1")
$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$jobScript = Join-Path $repo "scripts\job-object.ps1"
if (-not (Test-Path $jobScript)) {
    Write-Host "the job object script is missing, so the experiment cannot be run"
    exit 1
}

# The experiment proves the mechanism; this proves the suite is wired to it. Round 237 has the precedent: `check-kinds`
# existed, was proven failable and listed in the ledger, and `check-everything` had never heard of it. A job object that
# `check-everything` does not install is the same defect one level down.
$runnerText = Get-Content -LiteralPath (Join-Path $repo "scripts\check-everything.ps1") -Raw -Encoding UTF8
$unwired = @()
if ($runnerText -notmatch 'job-object\.ps1') { $unwired += "check-everything never loads scripts/job-object.ps1" }
if ($runnerText -notmatch 'Enter-KillOnCloseJob') { $unwired += "check-everything never calls Enter-KillOnCloseJob" }
if ($unwired.Count -gt 0) {
    $unwired | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "the suite does not install the job object this check proved"
    exit 1
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-orphans-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null

# A runner that starts a long sleep and then waits to be killed. `-UseJob` is the only difference between the two arms.
$runnerPath = Join-Path $work "runner.ps1"
$template = @'
param([string]$PidFile, [string]$StatusFile, [switch]$UseJob)

if ($UseJob) {
    . "__JOB_SCRIPT__"
    if (-not (Enter-KillOnCloseJob)) {
        [System.IO.File]::WriteAllText($StatusFile, "job-failed")
        exit 3
    }
}

$probe = Start-Process -FilePath "powershell.exe" -PassThru -NoNewWindow -ArgumentList @(
    "-NoProfile", "-Command", "Start-Sleep -Seconds 300")
[System.IO.File]::WriteAllText($PidFile, [string]$probe.Id)
[System.IO.File]::WriteAllText($StatusFile, "started")
Start-Sleep -Seconds 300
'@
$runner = $template.Replace('__JOB_SCRIPT__', $jobScript)
Set-Content -LiteralPath $runnerPath -Value $runner -Encoding UTF8

function Invoke-Experiment([bool]$useJob) {
    $label = if ($useJob) { "job" } else { "control" }
    $pidFile = Join-Path $work ("probe-" + $label + ".pid")
    $statusFile = Join-Path $work ("runner-" + $label + ".status")
    $arguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $runnerPath,
                   "-PidFile", $pidFile, "-StatusFile", $statusFile)
    if ($useJob) { $arguments += "-UseJob" }

    $runnerProcess = Start-Process -FilePath "powershell.exe" -PassThru -NoNewWindow -ArgumentList $arguments
    $deadline = (Get-Date).AddSeconds(20)
    while (-not (Test-Path $pidFile) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path $pidFile)) {
        try { Stop-Process -Id $runnerProcess.Id -Force -ErrorAction SilentlyContinue } catch { }
        return @{ label = $label; reached = $false; why = "the runner never started its child within 20 seconds" }
    }

    $probeId = [int]((Get-Content -LiteralPath $pidFile -Raw).Trim())
    $status = if (Test-Path $statusFile) { (Get-Content -LiteralPath $statusFile -Raw).Trim() } else { "missing" }
    if (-not (Get-Process -Id $probeId -ErrorAction SilentlyContinue)) {
        try { Stop-Process -Id $runnerProcess.Id -Force -ErrorAction SilentlyContinue } catch { }
        return @{ label = $label; reached = $false; why = "the child was already gone before the runner was killed" }
    }

    Stop-Process -Id $runnerProcess.Id -Force -ErrorAction SilentlyContinue

    $survived = $false
    if ($useJob) {
        # The job kills its members as the handle closes, which is immediate; ten seconds is patience, not a claim.
        $deadline = (Get-Date).AddSeconds(10)
        while ((Get-Date) -lt $deadline) {
            if (-not (Get-Process -Id $probeId -ErrorAction SilentlyContinue)) { break }
            Start-Sleep -Milliseconds 100
        }
        $survived = [bool](Get-Process -Id $probeId -ErrorAction SilentlyContinue)
    } else {
        Start-Sleep -Seconds 2
        $survived = [bool](Get-Process -Id $probeId -ErrorAction SilentlyContinue)
    }

    # A control probe is meant to survive; either way it must not be left behind for the next run to sweep.
    try { Stop-Process -Id $probeId -Force -ErrorAction SilentlyContinue } catch { }

    return @{ label = $label; reached = $true; status = $status; probeId = $probeId; survived = $survived }
}

$control = Invoke-Experiment $false
$withJob = Invoke-Experiment $true
$failures = @()

if (-not $control.reached) {
    $failures += ("the control run could not be performed: " + $control.why)
} else {
    Write-Host ("  without a job object: the child {0}" -f $(if ($control.survived) {
        "survived the killed runner, as it must"
    } else {
        "died with the killed runner, so this check cannot tell a job object from luck"
    }))
    if (-not $control.survived) { $failures += "the control child died with its runner, so the experiment cannot see survival" }
}

if (-not $withJob.reached) {
    $failures += ("the job-object run could not be performed: " + $withJob.why)
} else {
    Write-Host ("  with a job object: the child {0}" -f $(if ($withJob.survived) {
        "is still running after the runner was killed"
    } else {
        "died with the killed runner"
    }))
    if ($withJob.status -ne "started") { $failures += ("the job-object runner did not get as far as starting its child (status: " + $withJob.status + ")") }
    if ($withJob.survived) { $failures += "a killed runner left its child running" }
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a killed run can leave processes behind"
    exit 1
}
Write-Host "a killed runner takes the child it started, and the same runner without the job object does not"
