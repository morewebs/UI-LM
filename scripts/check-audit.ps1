# Audits every recording: the frames and the pages they were read into must agree.
#
# `ui-lm-capture audit` checks two things nothing else does:
#
#   * a frame must be self-consistent — a masked span inside the screen, and no masking while the
#     application's event stream was still reporting behind the change;
#   * every page row must say what the screen says — a page that names a row the screen does not
#     have, or reads a row differently from the source, is a page that lies about its own evidence.
#
# The second is the sharper one and it costs nothing to run. It was written before the pane container
# and is the only check that compares a page against the cells it came from, row by row.
#
#   powershell -File scripts/check-audit.ps1
#
# Exit code 1 if any recording reports a violation.

param(
    [string[]]$Roots = @("eval", "cases")
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$resolved = @()
foreach ($root in $Roots) {
    $full = if ([System.IO.Path]::IsPathRooted($root)) { $root } else { Join-Path $repo $root }
    if (Test-Path $full) { $resolved += $full }
}
                     if ($resolved.Count -eq 0) {
    Write-Host "no recordings found, so nothing was audited"
    exit 1
}

$sessions = @()
foreach ($root in $resolved) {
    $sessions += Get-ChildItem -LiteralPath $root -Recurse -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName "session.json") }
}

$auditArgs = @("audit")
foreach ($session in $sessions) { $auditArgs += @("--session", $session.FullName) }
$out = & $capture @auditArgs 2>&1
$code = $LASTEXITCODE
$lines = @($out | Where-Object { "$_".Trim() })
$violations = @($lines | Where-Object { "$_" -match "violation" -and "$_" -notmatch "no violations" })

Write-Host ("recordings audited: {0}" -f $sessions.Count)
Write-Host ("{0}" -f (($lines | Select-Object -Last 1) -join ""))

if ($code -ne 0 -or $violations.Count -gt 0) {
    $lines | Select-Object -First 8 | ForEach-Object { Write-Host ("  " + "$_".Trim()) }
    Write-Host "a recording contradicts itself"
    exit 1
}
Write-Host "every recording is consistent with itself"
