# Records screens the model has never been trained on: failures, not overlays.
#
# Every scenario in the training mix is the application working normally. This records it
# going wrong — a command that fails, a file that is not there, a long log, a failing test —
# because that is where a screen stops looking like the ones already recorded.
#
#   powershell -File scripts/build-unseen.ps1 [-Out DIR]

param(
    [string]$Out = "eval/unseen",
    [string]$Grok = "grok.exe"
)

$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }
if (-not (Get-Command $Grok -ErrorAction SilentlyContinue)) { throw "$Grok is not on PATH" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-unseen-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
if (-not [System.IO.Path]::IsPathRooted($Out)) { $Out = Join-Path $repo $Out }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue
$env:TERM = "xterm-256color"
$env:COLORTERM = "truecolor"

$scenarios = @(
    # A command that cannot succeed: the error path, which no training screen holds.
    @{ name = "unseen-fail";  keys = @("6000 \r", "4000 run a command that fails and show me the error\r", "16000 \r", "2500 /exit\r"); stop = 30000 }
    # A file that is not there.
    @{ name = "unseen-nofile"; keys = @("6000 \r", "4000 open missing.txt and show me what is in it\r", "14000 \r", "2500 /exit\r"); stop = 28000 }
    # Several tool calls in one turn: a long scrolling log rather than one result.
    @{ name = "unseen-longlog"; keys = @("6000 \r", "4000 list every file here, then read notes.txt, then run git status\r", "22000 \r", "2500 /exit\r"); stop = 36000 }
    # A failing test run: output that is neither a diff nor a list.
    @{ name = "unseen-tests"; keys = @("6000 \r", "4000 run python -m pytest and tell me what failed\r", "18000 \r", "2500 /exit\r"); stop = 32000 }
)

$recorded = @()
foreach ($scenario in $scenarios) {
    $keys = Join-Path $work ($scenario.name + ".keys")
    Set-Content -LiteralPath $keys -Value $scenario.keys -Encoding ASCII
    Write-Host "recording $($scenario.name)..."
    $project = Join-Path $work ("project-" + $scenario.name)
    New-Item -ItemType Directory -Force -Path $project | Out-Null
    Set-Content -LiteralPath (Join-Path $project "notes.txt") -Value @("alpha", "beta") -Encoding ASCII
    Set-Content -LiteralPath (Join-Path $project "broken.py") -Value @("def add(a, b):", "    return a - b", "", "def test_add():", "    assert add(1, 1) == 2") -Encoding ASCII
    $sessionOut = Join-Path $Out $scenario.name
    if (Test-Path $sessionOut) { Remove-Item -LiteralPath $sessionOut -Recurse -Force }
    & $capture record --rows 40 --cols 120 --out $sessionOut --cwd $project --stop-after $scenario.stop `
        --script $keys -- $Grok --cwd $project *> $null
    if (Test-Path (Join-Path $sessionOut "session.json")) { $recorded += $sessionOut }
}
if ($recorded.Count -eq 0) { throw "nothing was recorded" }

# The judgement of these screens is the model against the floor, per session, because a
# single average over four different kinds of failure would hide which kind broke.
foreach ($session in $recorded) {
    $line = (& $capture score --session $session --model (Join-Path $repo "models\page.json")) -join ' '
    "{0,-14} {1}" -f (Split-Path $session -Leaf), $line.Trim()
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
