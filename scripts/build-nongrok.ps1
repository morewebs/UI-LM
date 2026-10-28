# Records screens from programs that are not the application the model was trained on.
#
# The model was trained on Grok Build's screens. A model that only works there has learned
# Grok, not screens, so this records entirely different programs — and draws them the way a
# terminal application draws: a limit line of its own, a bordered panel, tool lines outside
# the box, a multi-line edit report, a column table, prose.
#
#   powershell -File scripts/build-nongrok.ps1 [-Out DIR]

param(
    [string]$Out = "eval/nongrok"
)

$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-nongrok-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work, $Out | Out-Null
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue
$env:TERM = "xterm-256color"
$env:COLORTERM = "truecolor"

# Each program draws one kind of terminal furniture. Written with the same glyphs a real
# application uses, because that is what the reader recognises.
$programs = [ordered]@{
    "banner" = @'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
line("4.2K / 900K | Storage")
line("")
line("Inventory report")
line("")
time.sleep(3)
'@
    "panel" = @'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
line("\u256d\u2500 Inventory \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u256e")
line("\u2502 Apples      12                        \u2502")
line("\u2502 Pears        7                        \u2502")
line("\u2502 Monday                         Ctrl+M \u2502")
line("\u2502 Tuesday                        /tuesday\u2502")
line("\u2570\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u256f")
line("")
time.sleep(3)
'@
    "report" = @'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
line("\u2666 Read inventory.csv")
line("\u2666 Edited inventory.csv   +2 -2")
line("")
line("Changes to inventory.csv")
line("")
line("  - Apples      12")
line("  + Apples      14")
line("  - Pears        7")
line("  + Pears        9")
line("")
time.sleep(3)
'@
    "table" = @'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
line("Item        Count    Price")
line("--------------------------")
line("Apples         12     1.20")
line("Pears           7     0.95")
line("Cherries       41     3.10")
line("")
time.sleep(3)
'@
    "prose" = @'
import sys, time
out = sys.stdout.buffer
def line(t):
    out.write(t.encode("utf-8") + b"\r\n"); out.flush()
line("Traceback (most recent call last):")
line("  File \"main.py\", line 4, in <module>")
line("KeyError: 'apples'")
line("")
line("The report could not be produced.")
line("")
time.sleep(3)
'@
}

$recorded = @()
foreach ($name in $programs.Keys) {
    $path = Join-Path $work ("$name.py")
    Set-Content -LiteralPath $path -Value $programs[$name] -Encoding UTF8
    $session = Join-Path $Out $name
    if (Test-Path $session) { Remove-Item -LiteralPath $session -Recurse -Force }
    & $capture record --rows 18 --cols 46 --out $session --stop-after 4500 -- python $path *> $null
    if (Test-Path (Join-Path $session "session.json")) { $recorded += $session }
}

Write-Host ("recorded {0} session(s) into {1}" -f $recorded.Count, (Resolve-Path $Out))

# What the reader found in them, so a weak evaluation set is visible rather than assumed.
$kinds = @{}
foreach ($session in $recorded) {
    $page = (& $capture page --session $session) -join ''
    foreach ($match in [regex]::Matches($page, '"component": "(\w+)"')) {
        $kind = $match.Groups[1].Value
        $kinds[$kind] = 1 + [int]$kinds[$kind]
    }
}
$kinds.GetEnumerator() | Sort-Object Name | ForEach-Object { Write-Host ("  {0,-10} x{1}" -f $_.Key, $_.Value) }

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
