# Downloads the published release and uses it the way a stranger would.
#
# Round 49 found two defects no amount of scoring could see: `summarize` rejected `--session`,
# and the archive shipped no model, so `score` could not run from a download at all. Both would
# have been caught by this.
#
#   powershell -File scripts/check-release.ps1
#   powershell -File scripts/check-release.ps1 -Tag v0.4.0
#
# Exit code 1 if the download is missing anything or a command fails.

param(
    [string]$Tag = "latest",
    # A recorded session to score with the downloaded model. Without one, the check still
    # verifies that every command exists and that the model travelled.
    [string]$Session = ""
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-release-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$failures = @()

& gh release download $Tag --pattern "ui-lm-Windows-X64.zip" --dir $work --repo morewebs/UI-LM 2>&1 | Out-Null
$zip = Get-ChildItem -LiteralPath $work -Filter "*.zip" | Select-Object -First 1
if (-not $zip) {
    Write-Host "the release had no Windows archive"
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}
Expand-Archive -LiteralPath $zip.FullName -DestinationPath $work -Force

$capture = Get-ChildItem -LiteralPath $work -Recurse -Filter "ui-lm-capture.exe" | Select-Object -First 1
$model = Get-ChildItem -LiteralPath $work -Recurse -Filter "page.json" | Select-Object -First 1
# Printed every run: the sizes and presence of what a stranger would actually receive.
Write-Host ("archive: {0:N0} KB, capture: {1}, model shipped: {2}" -f ($zip.Length / 1KB), [bool]$capture, [bool]$model)

if (-not $capture) { $failures += "the archive held no ui-lm-capture.exe" }
if (-not $model) { $failures += "the archive shipped no models/page.json, so a download cannot score a screen" }

if ($capture) {
    $help = (& $capture.FullName --help 2>&1) -join " "
    if ($help -notmatch "record") { $failures += "the downloaded binary does not answer --help" }

    # Every command the README tells a newcomer to run must exist in the download.
    foreach ($command in @("record", "page", "score", "tui", "render", "summarize")) {
        $answer = (& $capture.FullName $command --help 2>&1 | Select-Object -First 1) -join " "
        if ($answer -match "unrecognized|error:") { $failures += "the download has no '$command' command" }
    }
}

# A session to score: the caller's, or the newest thing recorded on this machine.
if (-not $Session) {
    $candidate = Get-ChildItem -LiteralPath (Join-Path $repo "eval") -Recurse -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path (Join-Path $_.FullName "session.json") } | Select-Object -First 1
    if ($candidate) { $Session = $candidate.FullName }
}
if ($Session -and $capture -and $model) {
    $line = (& $capture.FullName score --session $Session --model $model.FullName 2>&1 | Select-Object -First 1) -join " "
    Write-Host ("downloaded build, on a real session: {0}" -f $line.Trim())
    if ($line -notmatch "model \d") { $failures += "the downloaded build could not score a session" }
} else {
    Write-Host "no recorded session on this machine, so scoring was not exercised"
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "the download works the way a stranger would use it"
