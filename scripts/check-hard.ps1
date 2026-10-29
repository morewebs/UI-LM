# Reports the screens this project cannot yet read perfectly.
#
# These are real recordings, kept outside the evaluation sets because they fail. Measuring them is
# the point: a known gap that nothing looks at becomes an unknown gap within a month, and the whole
# discipline here is that claims and evidence are the same document.
#
#   powershell -File scripts/check-hard.ps1
#
# Exit code 0 always — these are not gates, they are a scoreboard. What must not happen is that one
# of them silently gets *worse*, so each is compared against the figure recorded in
# docs/reports/2026-10-28-model.md and a regression is reported loudly.

param(
    [string]$Model = "models/page.json"
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }
$model = if ([System.IO.Path]::IsPathRooted($Model)) { $Model } else { Join-Path $repo $Model }
$cases = Join-Path $repo "cases\hard"

if (-not (Test-Path $cases)) {
    Write-Host "no hard cases on this machine (they are recorded locally and never committed)"
    exit 0
}

# The figures these screens reached when last measured. A drop here means something regressed.
$expected = @{
    "wsl-agent-in-pane"  = 100.0
    "wsl-agent-in-split" = 100.0
    "wsl-two-agents-in-split" = 100.0
    "wsl-vim-in-split" = 100.0
    "wsl-blank-in-split" = 100.0
    "vim-floating-popup" = 100.0
    # Fixed: an advertisement panel whose border ends `╯  █` stopped being a panel once the scrollbar
    # column was not stripped, so the reader swallowed the rest of the screen into one block. Measured at
    # 99.7% before the fix and 100% after it; that is the number a regression would have to fall below.
    "freebuff-refusal" = 100.0
}

# How many components the busiest frame of each recording holds, measured after the tree decision of
# round 26. A page can score 100% and still be wrong: `cargo tree` was read as seventeen blocks at 100%,
# because the model copies the reader and both were wrong the same way. Fragmentation is invisible to a
# score, so the shape of the page is checked directly — a frame that suddenly holds twice the components
# it used to has been cut up, and somebody has to say why.
$ceiling = @{
    "wsl-agent-in-pane"        = 11
# Raised by one in round 43, with the reason: the scrollbar fix (round 33) lets a panel close when its
    # border is followed by the terminal's own column, so the content under such a panel is no longer
    # swallowed into it — one more component, and a truer page. A ceiling moves when a decision moves it,
    # never to make a run pass.
    "wsl-agent-in-split"       = 11
    "wsl-blank-in-split"       = 2
    "wsl-two-agents-in-split"  = 20
    "wsl-vim-in-split"         = 2
    "freebuff-refusal"         = 13
    # Was 1: the floating window was swallowed into the block around it. The box rules now see a heavy
    # border drawn mid-line, so the window is its own component and the floor fell from 100% to 75%.
    "vim-floating-popup"       = 3
}
$regressed = 0
foreach ($case in (Get-ChildItem -LiteralPath $cases -Directory)) {
    $line = (& $capture score --session $case.FullName --model $model 2>&1 | Select-Object -First 1) -join ""
    $share = 0.0
    if ($line -match "model ([\d.]+)%") { $share = [double]$Matches[1] }
# Score says the two pages agree; it says nothing about whether the page can be drawn back.
    # Two full panes agreed perfectly and round tripped at 23.8% for four rounds, because this
    # check only ever asked the first question.
    $trip = (& $capture tui --session $case.FullName --all 2>&1 | Select-Object -Last 1) -join ""
    $tripped = 0.0
    if ($trip -match "\(([\d.]+)%\)") { $tripped = [double]$Matches[1] }
    # The busiest frame, against the recorded ceiling.
    $busiest = 0
    foreach ($raw in (& $capture page --session $case.FullName --all 2>$null)) {
        $page = $raw | ConvertFrom-Json -ErrorAction SilentlyContinue
        if (-not $page) { continue }
        $count = @($page.createSurface.components | Where-Object { $_.component -ne "Column" }).Count
        if ($count -gt $busiest) { $busiest = $count }
    }
    if ($ceiling.ContainsKey($case.Name) -and $busiest -gt $ceiling[$case.Name]) {
        Write-Host ("  {0,-22} busiest frame holds {1} components, the ceiling is {2}" -f $case.Name, $busiest, $ceiling[$case.Name])
        $regressed++
    }
    $was = if ($expected.ContainsKey($case.Name)) { $expected[$case.Name] } else { $null }
    if ($tripped -lt 100.0) {
        Write-Host ("  {0,-22} draws back at {1:N1}% ({2})" -f $case.Name, $tripped, $trip.Trim())
        $regressed++
    }
    if ($null -eq $was) {
        Write-Host ("  {0,-22} {1,5:N1}%  (no expectation recorded)" -f $case.Name, $share)
    } elseif ($share -lt $was) {
        Write-Host ("  {0,-22} {1,5:N1}%  REGRESSED (was {2:N1}%)" -f $case.Name, $share, $was)
        $regressed++
    } else {
        Write-Host ("  {0,-22} {1,5:N1}%  (was {2:N1}%)" -f $case.Name, $share, $was)
    }
}

if ($regressed -gt 0) {
    Write-Host ""
    Write-Host "a known-hard screen got worse; that is a regression, not a gap"
    exit 1
}
Write-Host "known-hard screens are no worse than last measured"
