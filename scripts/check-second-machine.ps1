# Records a real program on whatever machine runs it, then asks the reader to describe the screen.
#
# Every recording in this repository was made on one Windows laptop. The recorder ships for three platforms and the
# test suite runs on three, but nothing has ever *recorded* anywhere else — so "the reader works on a screen from
# another machine" has been an argument, not a measurement.
#
# This is the smallest honest version of that measurement: a program that exists on the runner, a screen it draws,
# and the round trip for the page the reader builds from it. It runs on the development machine too, which is how
# the check itself was shown to work before CI ever saw it.
#
#   powershell -File scripts/check-second-machine.ps1        (or pwsh -File on Linux and macOS)
#
# Exit code 1 if the machine is not recorded, the page does not round trip, or the screen is empty.

param(
    [int]$Rows = 30,
    [int]$Cols = 100,
    [int]$Seconds = 12000
)

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) {
    $capture = Join-Path $repo "target/debug/ui-lm-capture"
}
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-machine-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null

# A program every platform has, drawing a screen wide enough to have columns in it.
$isWindows = $PSVersionTable.Platform -eq "Win32NT" -or $env:OS -eq "Windows_NT"
if ($isWindows) {
    $command = "cmd.exe"
    $arguments = @("/c", "dir /s /b C:\Windows\System32\*.dll")
} else {
    $command = "sh"
    $arguments = @("-c", "ls -l /usr/bin | head -60")
}

$session = Join-Path $work "s"
& $capture record --rows $Rows --cols $Cols --out $session --stop-after $Seconds -- $command @arguments 2>&1 |
    Select-Object -Last 1 | ForEach-Object { Write-Host ("  " + $_) }

if (-not (Test-Path (Join-Path $session "session.json"))) {
    Write-Host "the machine was not recorded: no session was written"
    exit 1
}

$frames = @(Get-Content -LiteralPath (Join-Path $session "frames.jsonl") | Where-Object { $_.Trim() }).Count
if ($frames -eq 0) {
    Write-Host "the machine was recorded but holds no frame, so nothing was measured"
    exit 1
}

$roundTrip = (& $capture tui --session $session --all 2>&1 | Select-Object -Last 1) -join ""
Write-Host ("  " + $roundTrip)

$score = (& $capture score --session $session --model (Join-Path $repo "models\page.json") 2>&1 | Select-Object -First 1) -join ""
Write-Host ("  " + $score)

# A blank screen round trips at 100% — `0/0 line(s) of content came back (100.0%)` is what an empty recording
# prints. The first version of this check accepted that, which made it a check that could pass on nothing.
if ($roundTrip -notmatch "(\d+)/(\d+) line") {
    Write-Host "the round trip printed no line count, so nothing was measured"
    exit 1
}
$cameBack = [int]$Matches[1]
if ($cameBack -eq 0) {
    Write-Host "the screen holds no lines, so a perfect round trip would prove nothing"
    exit 1
}
if ($roundTrip -notmatch "\(100\.0%\)") {
    Write-Host "this machine's screen did not round trip exactly"
    exit 1
}

$platform = if ($isWindows) { "Windows" } else { (Get-Content /etc/os-release -EA SilentlyContinue | Where-Object { $_ -match '^PRETTY_NAME=' } | Select-Object -First 1) -replace 'PRETTY_NAME="?|"$', '' }
Write-Host ("a screen recorded on {0} round trips exactly ({1} frame(s))" -f $(if ($platform) { $platform } else { "this machine" }), $frames)

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
exit 0
