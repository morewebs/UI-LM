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
    "whiptail-dialog" = 100.0
    "whiptail-menu" = 100.0
    # A counterexample, not a positive case: a borderless popup is a block, and this recording exists to fail
    # loudly if a future predicate starts calling one a floating window.
    "vim-completion-popup" = 100.0
    "every-kind-one-screen" = 100.0
    # The orchestration shapes, promoted from `eval/apps` in round 100: one agent beside a shell, an agent
    # working in a pane, two agents on one screen, and a divider that moves mid-session. They were recorded
    # once; a hard case is what stops them being lost.
    "agent-beside-shell" = 100.0
    "pane-conversation" = 100.0
    "two-agents-one-screen" = 100.0
    "pane-resize" = 100.0
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
# Kinds a case must actually produce. A ceiling counts components; it cannot say what they are, and the
# floating window proved it: three blocks would satisfy the popup's ceiling of 3 while the `Frame` the case
# exists for had silently gone missing. Step 5 of `docs/adding-a-component.md`: a new kind is proven by
# breaking it and watching a named check fail.
# Kinds a screen must NOT claim. The other tables say what a case proves is there; this one says what a case
# proves is absent, which is the only way to guard a boundary — the `Frame` predicate must keep declining vim's
# borderless completion popup, and a rule that starts accepting it should fail here rather than look like progress.
$forbidden = @{
    "vim-completion-popup" = @("Frame")
    # Two screens of shell output and nothing else. `Meter`, `Thought`, `ToolCall`, `Diff` and `Frame` are things agents
    # draw; a reader that invents one of them here has stopped reading the screen (round 216). Its score and ceiling
    # would both survive that: two blocks of made-up kinds agree with themselves perfectly.
    "pane-resize"          = @("Meter", "Thought", "ToolCall", "Diff", "Frame", "Table")
    "agent-beside-shell"   = @("Diff", "Frame")
}

$required = @{
    # Each entry is a kind the case exists for. Breaking the predicate that produces it must fail this check,
    # and `docs/checks.md` records the message each break produced.
    "vim-floating-popup"     = @("Frame")
    # Added in round 215: this case exists because a centred dialog is a `Frame`, and the table never said so — its
    # ceiling protects the count and its score protects the agreement, and nothing protected the kind.
    "whiptail-menu"          = @("Frame")
    "whiptail-dialog"        = @("Frame")
    "every-kind-one-screen"  = @("Block", "ToolCall", "Diff", "Meter")
    "pane-conversation"      = @("Block", "Meter", "ToolCall")
    "two-agents-one-screen"  = @("Block", "Meter", "ToolCall")
    "freebuff-refusal"       = @("Thought")
    "wsl-agent-in-pane"      = @("Meter", "ToolCall")
    "wsl-agent-in-split"     = @("Meter", "Thought", "ToolCall")
    "wsl-two-agents-in-split" = @("Meter", "Thought", "ToolCall")
}
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
    # 1 until ADR 0006's second predicate lands: whiptail draws its window with single rules, and the
    # heavy-rule predicate correctly does not claim it.
    "whiptail-dialog"          = 1
    "every-kind-one-screen"    = 4
    "agent-beside-shell"       = 2
    "pane-conversation"        = 9
    "two-agents-one-screen"    = 15
    "pane-resize"              = 2
    # Both of these hold one component and that is the judgement being protected: a popup or a menu read as one
    # component per row would still score 100% and would be wrong. They were added without ceilings, which meant the
    # only thing checking them was the score (round 214).
    "whiptail-menu"            = 1
    "vim-completion-popup"     = 1
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
    $kinds = @{}
    foreach ($raw in (& $capture page --session $case.FullName --all 2>$null)) {
        $page = $raw | ConvertFrom-Json -ErrorAction SilentlyContinue
        if (-not $page) { continue }
        $count = @($page.createSurface.components | Where-Object { $_.component -ne "Column" }).Count
        if ($count -gt $busiest) { $busiest = $count }
        foreach ($component in $page.createSurface.components) { $kinds[$component.component] = $true }
    }
    if ($required.ContainsKey($case.Name)) {
        foreach ($kind in $required[$case.Name]) {
            if (-not $kinds.ContainsKey($kind)) {
                Write-Host ("  {0,-22} no {1} component, and this case exists for it" -f $case.Name, $kind)
                $regressed++
            }
        }
    }
    if ($forbidden.ContainsKey($case.Name)) {
        foreach ($kind in $forbidden[$case.Name]) {
            if ($kinds.ContainsKey($kind)) {
                Write-Host ("  {0,-22} holds a {1}, and this screen is the counterexample for it" -f $case.Name, $kind)
                $regressed++
            }
        }
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
