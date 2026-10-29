# The README and the binary must agree about what the binary can do.
#
# This found four undocumented subcommands — `audit`, `fields`, `disagree`, `baseline` — including the one
# that found three duplicate ids in the model's pages. A reader who only reads the README could not have
# run it.

param()

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
$readme = Join-Path $repo "README.md"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }
if (-not (Test-Path $readme)) { Write-Host "no README, so nothing was compared"; exit 1 }

$text = Get-Content -LiteralPath $readme -Raw -Encoding UTF8
$commands = @(& $capture --help 2>&1 | Select-String -Pattern '^  (\w+)' | ForEach-Object { ($_.Line.Trim() -split '\s+')[0] }) |
    Where-Object { $_ -and $_ -ne "help" }

$missing = @($commands | Where-Object { $text -notmatch [regex]::Escape($_) })
Write-Host ("  {0} command(s), {1} documented" -f $commands.Count, ($commands.Count - $missing.Count))
if ($commands.Count -lt 10) {
    Write-Host "the binary reports fewer commands than it has, so this check is reading nothing"
    exit 1
}
if ($missing.Count -gt 0) {
    $missing | ForEach-Object { Write-Host ("  the README never mentions: " + $_) }
    Write-Host "the README does not describe everything the binary can do"
    exit 1
}
Write-Host "every command in the binary is mentioned in the README"