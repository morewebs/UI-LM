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

# Every script the documents tell you to run must exist. A renamed check leaves the instruction behind,
# and the instruction is the only thing a reader has.
$docs = @("README.md", "HANDOFF.md", "docs\checks.md", "docs\reports\2026-10-28-state-of-the-project.md", "docs\reports\2026-10-29-month-in-review.md")
$named = @()
foreach ($doc in $docs) {
    $path = Join-Path $repo $doc
    if (-not (Test-Path $path)) { continue }
    $text = Get-Content -LiteralPath $path -Raw -Encoding UTF8
    foreach ($found in [regex]::Matches($text, 'scripts[/\\]([a-z0-9-]+\.ps1)')) { $named += $found.Groups[1].Value }
}
$named = $named | Sort-Object -Unique
$gone = @($named | Where-Object { -not (Test-Path (Join-Path $repo ("scripts\" + $_))) })
Write-Host ("  {0} script(s) named in the documents" -f $named.Count)
if ($named.Count -lt 5) {
    Write-Host "the documents name too few scripts for this to have checked anything"
    exit 1
}
if ($gone.Count -gt 0) {
    $gone | ForEach-Object { Write-Host ("  a document tells you to run " + $_ + ", which is not there") }
    Write-Host "a document names a script that does not exist"
    exit 1
}
Write-Host "every script the documents name exists"

# The handoff lists the CLI in one line. It was missing two commands until a reader checked by hand, so the
# line is checked here: every subcommand the binary has must appear in it.
$handoff = Join-Path $repo "HANDOFF.md"
if (Test-Path $handoff) {
    $text = Get-Content -LiteralPath $handoff -Raw -Encoding UTF8
    $line = $null
    foreach ($candidate in ($text -split "`n")) {
        if ($candidate -match '`record .+`') { $line = $candidate; break }
    }
    if ($null -eq $line) {
        Write-Host "the handoff no longer lists the commands, so nothing was compared"
        exit 1
    }
    $omitted = @($commands | Where-Object { $line -notmatch ('\b' + [regex]::Escape($_) + '\b') })
    if ($omitted.Count -gt 0) {
        $omitted | ForEach-Object { Write-Host ("  the handoff's command line omits: " + $_) }
        Write-Host "the handoff does not list every command"
        exit 1
    }
    Write-Host "the handoff lists every command"
}