# Audits every recording: a frame must be self-consistent, and every page row must match its screen.
#
# This is the only check that compares a page against the cells it came from, row by row. It found the
# pane a renderer had silently dropped, a duplicate component id, and a meter that claimed seven rows
# and filled one.
#
#   powershell -File scripts/check-audit.ps1

param(
    [string[]]$Roots = @("eval", "cases")
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$folders = @()
foreach ($root in $Roots) {
    $full = if ([System.IO.Path]::IsPathRooted($root)) { $root } else { Join-Path $repo $root }
    if (Test-Path $full) { $folders += $full }
}
if ($folders.Count -eq 0) { Write-Host "no recordings found"; exit 1 }

$sessions = @()
foreach ($folder in $folders) {
    $sessions += Get-ChildItem -LiteralPath $folder -Recurse -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName "session.json") }
}

# `audit` takes its directories after `--`. Passing `--session` is accepted and ignored, which is how a
# check spent three rounds auditing nothing and reporting "no violations".
$command = @("audit", "--") + @($sessions | ForEach-Object { $_.FullName })
$out = & $capture @command 2>&1
$code = $LASTEXITCODE
$lines = @($out | Where-Object { "$_".Trim() })

$counts = ($lines | Where-Object { "$_" -match "recording\(s\)" } | Select-Object -First 1) -join ""
if ($counts -notmatch "audit: (\d+) recording") { Write-Host "the audit said nothing about what it inspected"; exit 1 }
$inspected = [int]$Matches[1]

Write-Host $counts.Trim()
Write-Host (($lines | Select-Object -Last 1) -join "").Trim()

if ($code -ne 0) {
    $lines | Where-Object { "$_" -match "frame \d+:" } | Select-Object -First 6 |
        ForEach-Object { Write-Host ("  " + "$_".Trim()) }
    Write-Host ("{0} recording(s) contradict themselves" -f $inspected)
    exit 1
}
if ($inspected -lt $sessions.Count) {
    Write-Host ("the audit inspected {0} of {1} recordings" -f $inspected, $sessions.Count)
    exit 1
}
Write-Host "every recording is consistent with itself"