# Builds a local corpus of training pairs from several terminal programs.
#
# The corpus is never committed: `corpus/` is ignored, because a recorded screen carries
# the machine it was recorded on and a published corpus is a decision, not a default. This
# script records, exports with `--scrub`, and prints what it got so the numbers are known
# without opening the file.
#
#   pwsh scripts/build-corpus.ps1                  # the default scenarios
#   pwsh scripts/build-corpus.ps1 -Keep            # keep the recordings, not just the pairs
#
# Recording is slow on purpose: a frame exists only once a screen has settled, so each
# scenario waits for a real program to draw, react and stop.

param(
    [string]$OutDir = "corpus",
    [switch]$Keep,
    # The grok binary, when it should be part of the corpus.
    [string]$Grok = "grok.exe"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) {
    throw "build first: cargo build --workspace (looked for $capture)"
}

$scratch = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-corpus-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $scratch | Out-Null
$sessions = Join-Path $scratch "sessions"
New-Item -ItemType Directory -Force -Path $sessions | Out-Null
# A colour-capable terminal, and no inherited NO_COLOR: a screen recorded without colour is
# a screen with less in it.
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue
$env:TERM = "xterm-256color"
$env:COLORTERM = "truecolor"

# Each scenario: a name, a script of timed keystrokes, and the program to run.
$scenarios = @(
    @{ name = "grok-help";        keys = @("6000 \r", "4000 /help\r", "9000 \r", "2500 /exit\r");        stop = 24000; grok = $true }
    @{ name = "grok-session";     keys = @("6000 \r", "4000 /session-info\r", "15000 \r", "2500 /exit\r"); stop = 29000; grok = $true }
    @{ name = "grok-usage";       keys = @("6000 \r", "4000 /usage\r", "9000 \r", "2500 /exit\r");       stop = 24000; grok = $true }
    @{ name = "grok-context";     keys = @("6000 \r", "4000 /context\r", "9000 \r", "2500 /exit\r");      stop = 24000; grok = $true }
)

$recorded = @()
foreach ($scenario in $scenarios) {
    $sessionOut = Join-Path $sessions $scenario.name
    if ($scenario.grok -and -not (Get-Command $Grok -ErrorAction SilentlyContinue)) {
        Write-Host "skipping $($scenario.name): $Grok is not on PATH"
        continue
    }
    $keys = Join-Path $scratch ($scenario.name + ".keys")
    Set-Content -LiteralPath $keys -Value $scenario.keys -Encoding ASCII
    Write-Host "recording $($scenario.name)..."
    $project = Join-Path $scratch ("project-" + $scenario.name)
    New-Item -ItemType Directory -Force -Path $project | Out-Null
    Set-Content -LiteralPath (Join-Path $project "notes.txt") -Value @("alpha", "beta") -Encoding ASCII
    if ($scenario.grok) {
        & $capture record --rows 40 --cols 120 --out $sessionOut --cwd $project --stop-after $scenario.stop `
            --script $keys -- $Grok --cwd $project | Out-Null
    }
    if (Test-Path (Join-Path $sessionOut "session.json")) { $recorded += $sessionOut }
}

# Programs that are not the application under study, so the corpus is not one program's
# idea of what a screen looks like.
Write-Host "recording an ordinary command..."
$dirOut = Join-Path $sessions "cmd-dir"
& $capture record --rows 24 --cols 80 --out $dirOut --stop-after 3000 -- cmd /c "dir /b /s" | Out-Null
if (Test-Path (Join-Path $dirOut "session.json")) { $recorded += $dirOut }

Write-Host "recording a shell..."
$shOut = Join-Path $sessions "powershell-list"
& $capture record --rows 24 --cols 90 --out $shOut --stop-after 3000 -- powershell -NoProfile -Command "Get-ChildItem -Name" | Out-Null
if (Test-Path (Join-Path $shOut "session.json")) { $recorded += $shOut }

if ($recorded.Count -eq 0) { throw "nothing was recorded" }

# An output path means "here", not "wherever the shell was launched": resolve it against
# the repository, which is where a person running this expects to find it.
if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path $repo $OutDir }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$pairs = [System.IO.Path]::GetFullPath((Join-Path $OutDir "pairs.jsonl"))
Write-Host "writing pairs to $pairs"
if (Test-Path $pairs) { Remove-Item -LiteralPath $pairs -Force }
$arguments = @("dataset", "--out", $pairs, "--scrub")
foreach ($session in $recorded) { $arguments += @("--session", $session) }
& $capture @arguments | Out-Null
if ($LASTEXITCODE -ne 0) { throw "the export failed" }
# The export claiming success is not evidence that a file exists where this script will
# look for it. A silent miss is worse than a failure: it reports counts from a file nobody
# can find, which is exactly what happened before this check existed.
if (-not (Test-Path $pairs)) { throw "the export said it wrote $pairs and it is not there" }

# What the corpus holds, counted rather than assumed.
$lines = [System.IO.File]::ReadAllLines((Resolve-Path $pairs))
$components = @{}
foreach ($line in $lines) {
    $pair = $line | ConvertFrom-Json
    $page = $pair.target | ConvertFrom-Json
    foreach ($component in $page.createSurface.components) {
        $components[$component.component] = 1 + [int]$components[$component.component]
    }
}
Write-Host ""
Write-Host ("corpus: {0} session(s), {1} pair(s), {2:N0} KB" -f $recorded.Count, $lines.Count, ((Get-Item $pairs).Length / 1KB))
$components.GetEnumerator() | Sort-Object -Descending Name | ForEach-Object {
    Write-Host ("  {0,-10} x{1}" -f $_.Key, $_.Value)
}
$account = $env:USERNAME
if ($account) {
    $carrying = ($lines | Where-Object { $_ -match [regex]::Escape($account) } | Measure-Object).Count
    Write-Host ("pairs naming this account: {0} of {1}" -f $carrying, $lines.Count)
}

if ($Keep) {
    Write-Host "recordings kept at $sessions"
} else {
    Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
}
