# The gate every commit must pass: formatting, lints, tests. One exit code.
#
# This exists because the discipline was in my attention rather than in the tooling, and in round 101
# a commit went through while clippy and the tests were both failing. A gate that depends on someone
# remembering to look at three exit codes is not a gate.
#
#   powershell -File scripts/gate.ps1
#   powershell -File scripts/gate.ps1 -Commit "message"    # gate, then commit if it passed
#
# Exit code 1 if any check fails. With -Commit, nothing is committed unless all three pass.

param(
    [string]$Commit = ""
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

& cargo clippy --workspace --all-targets --quiet -- -D warnings *> $null
$clippy = $LASTEXITCODE
& cargo fmt --all --check *> $null
$format = $LASTEXITCODE
& cargo test --workspace --quiet *> $null
$tests = $LASTEXITCODE

function Report($name, $code) {
    if ($code -eq 0) {
        Write-Host ("  {0,-8} ok" -f $name)
    } else {
        Write-Host ("  {0,-8} FAILED (exit {1})" -f $name, $code)
    }
}
Report "clippy" $clippy
Report "fmt" $format
Report "test" $tests

$failed = 0
if ($clippy -ne 0) { $failed++ }
if ($format -ne 0) { $failed++ }
if ($tests -ne 0) { $failed++ }

if ($failed -gt 0) {
    Write-Host ""
    Write-Host "the gate is closed: $failed check(s) failed, nothing is committed"
    exit 1
}

if ($Commit -ne "") {
    & git add -A
    & git commit -q -m $Commit
    if ($LASTEXITCODE -ne 0) {
        Write-Host "the gate passed but the commit did not"
        exit 1
    }
    Write-Host ("committed: " + (& git rev-parse --short HEAD))
} else {
    Write-Host "the gate is open"
}
