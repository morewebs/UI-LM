# Records screens from programs that are not the application the model was trained on.
#
# The model was trained on Grok Build's screens. A model that only works there has learned
# Grok, not screens — so this records entirely different programs: a drawn panel, an
# edit-style output, a listing. Nothing here has been seen in training, and the shapes are
# deliberately unlike the training application's.

param(
    [string]$Out = "eval/nongrok"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-nongrok-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue
$env:TERM = "xterm-256color"
$env:COLORTERM = "truecolor"

$programs = @(
    # A drawn panel with a shortcut list and a meter: the shapes the model must recognise,
    # in an application it has never seen.
    @{ name = "panel";   script = "panel.py" },
    # An edit report, drawn as the minus and plus lines a diff is made of.
    @{ name = "edits";   script = "edits.py" },
    # An ordinary listing: mostly prose, included so the model is judged on easy screens too.
    @{ name = "listing"; script = "listing.py" }
)

$drawn = @{
    "panel.py" = @'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
line("\u256d\u2500 Inventory \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u256e")
line("\u2502 3.4K / 900K | Disk                        \u2502")
line("\u2502 Apples      12                            \u2502")
line("\u2502 Pears        7                            \u2502")
line("\u2502 Monday                         Ctrl+M     \u2502")
line("\u2502 Tuesday                        /tuesday   \u2502")
line("\u2570\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u256f")
line("")
time.sleep(3)
'@
    "edits.py" = @'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
line("Changes to inventory.txt")
line("")
line("  - Apples      12")
line("  + Apples      14")
line("")
line("2 changed")
time.sleep(3)
'@
    "listing.py" = @'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
for name in ("alpha.txt", "beta.txt", "gamma.txt"):
    line(name)
line("")
line("3 files")
time.sleep(3)
'@
}

$recorded = @()
foreach ($program in $programs) {
    $path = Join-Path $work $program.script
    Set-Content -LiteralPath $path -Value $drawn[$program.script] -Encoding UTF8
    $session = Join-Path $Out $program.name
    & $capture record --rows 20 --cols 60 --out $session --stop-after 4500 -- python $path | Out-Null
    if (Test-Path (Join-Path $session "session.json")) { $recorded += $session }
}

# A listing from the operating system itself: no Python, no drawn shapes.
$os = Join-Path $Out "os-listing"
& $capture record --rows 24 --cols 80 --out $os --stop-after 3000 -- cmd /c "dir /b" | Out-Null
if (Test-Path (Join-Path $os "session.json")) { $recorded += $os }

Write-Host ("recorded {0} session(s) into {1}" -f $recorded.Count, (Resolve-Path $Out))
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
