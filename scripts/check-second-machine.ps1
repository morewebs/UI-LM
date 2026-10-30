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

# Two screens every POSIX machine has, chosen because they differ in kind: a listing is columns of text, and `top`
# is a header plus columns. Text differs between machines and always will — different files, different processes —
# so what is compared is the **shape**: which kinds the page claims and how many panes it splits into.
$isWindows = $PSVersionTable.Platform -eq "Win32NT" -or $env:OS -eq "Windows_NT"
$scenarios = @(
    @{ Name = "a directory listing"; Command = "ls -l /usr/bin | head -40; sleep 1"; Forbidden = @("Meter", "Thought", "ToolCall", "Diff", "Frame"); Panes = 0 },
    @{ Name = "a process monitor";   Command = "top -bn1 | head -25; sleep 1";     Forbidden = @("Meter", "Thought", "ToolCall", "Diff", "Frame"); Panes = 0 },
    # `Frame` was the kind with one producer and three examples on this machine, so the question worth asking of a
    # runner is not whether it reads text but whether the *predicate travels*: a whiptail dialog is a titled box
    # floating inside the screen, and it must be a `Frame` here as it is at home. If whiptail is not installed the
    # scenario is skipped and says so, rather than passing on an empty recording.
    @{ Name = "a dialog box";        Command = "whiptail --title 'Pick a file' --menu 'Choose one:' 20 60 5 alpha 'the first' beta 'the second' 2>/dev/null; sleep 1"; Forbidden = @(); Panes = 0; Requires = "whiptail"; Expects = "Frame" }
)

$failures = @()
foreach ($scenario in $scenarios) {
    if ($isWindows) {
        $command = "wsl.exe"
        $arguments = @("-e", "sh", "-c", $scenario.Command)
    } else {
        $command = "sh"
        $arguments = @("-c", $scenario.Command)
    }

    if ($scenario.ContainsKey("Requires")) {
        # The probe takes the same shape as the recording below it: on Windows the command is `wsl.exe -e sh -c`,
        # on Linux it is `sh -c`. Using the WSL form everywhere made this say "not installed" on a runner where the
        # package had just been installed.
        $probe = "command -v " + $scenario.Requires + " > /dev/null 2>&1 && echo yes"
        $present = if ($isWindows) {
            & $command -e sh -c $probe 2>$null
        } else {
            & $command -c $probe 2>$null
        }
        if (-not ($present -match "yes")) {
            Write-Host ("  {0,-22} skipped: {1} is not installed on this machine" -f $scenario.Name, $scenario.Requires)
            continue
        }
    }

    $session = Join-Path $work ("s-" + ($scenario.Name -replace ' ', '-'))
    & $capture record --rows $Rows --cols $Cols --out $session --stop-after $Seconds -- $command @arguments 2>&1 |
        Select-Object -Last 1 | Out-Null

    if (-not (Test-Path (Join-Path $session "session.json"))) {
        $failures += ("  " + $scenario.Name + " -> the machine was not recorded: no session was written")
        continue
    }

    $frames = @(Get-Content -LiteralPath (Join-Path $session "frames.jsonl") | Where-Object { $_.Trim() }).Count
    if ($frames -eq 0) {
        $failures += ("  " + $scenario.Name + " -> recorded but holds no frame, so nothing was measured")
        continue
    }

    $roundTrip = (& $capture tui --session $session --all 2>&1 | Select-Object -Last 1) -join ""
    # A blank screen round trips at 100% — `0/0 line(s) of content came back (100.0%)` is what an empty recording
    # prints. The first version of this check accepted that, which made it a check that could pass on nothing.
    if ($roundTrip -notmatch "(\d+)/(\d+) line") {
        $failures += ("  " + $scenario.Name + " -> the round trip printed no line count, so nothing was measured")
        continue
    }
    $cameBack = [int]$Matches[1]
    if ($cameBack -eq 0) {
        $failures += ("  " + $scenario.Name + " -> the screen holds no lines, so a perfect round trip proves nothing")
        continue
    }
    if ($roundTrip -notmatch "\(100\.0%\)") {
        $failures += ("  " + $scenario.Name + " -> " + $roundTrip)
        continue
    }

    $kinds = @{}
    $paneCount = 0
    foreach ($raw in (& $capture page --session $session --all 2>$null)) {
        $page = $raw | ConvertFrom-Json
        foreach ($component in $page.createSurface.components) { $kinds[$component.component] = 1 + $kinds[$component.component] }
        if (@($page.createSurface.panes | Where-Object { $null -ne $_ }).Count -gt 0) { $paneCount++ }
    }
    $shape = ($kinds.GetEnumerator() | Sort-Object Key | ForEach-Object { $_.Key + "=" + $_.Value }) -join ", "
    Write-Host ("  {0,-22} {1,2} frame(s)  kinds: {2}  ({3})" -f $scenario.Name, $frames, $shape, $roundTrip.Trim())

    if ($scenario.ContainsKey("Expects") -and -not $kinds.ContainsKey($scenario.Expects)) {
        $failures += ("  " + $scenario.Name + " -> was read as " + $shape + ", and this screen is a " + $scenario.Expects)
        continue
    }
    if (-not $scenario.ContainsKey("Expects") -and -not $kinds.ContainsKey("Block")) {
        $failures += ("  " + $scenario.Name + " -> was not read as a block, so the shape is not what it is")
        continue
    }
    foreach ($unexpected in $scenario.Forbidden) {
        if ($kinds.ContainsKey($unexpected)) {
            $failures += ("  " + $scenario.Name + " -> a plain screen was read as " + $unexpected + ", which no machine's screen of it is")
        }
    }
    $expectedPanes = [int]$scenario.Panes
    if ($paneCount -gt $expectedPanes) {
        $failures += ("  " + $scenario.Name + " -> split into " + $paneCount + " frame(s) with panes, and it has none")
    }
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host $_ }
    Write-Host "a screen from this machine was not read the way a screen of it reads anywhere"
    exit 1
}
$platform = if ($isWindows) { "Windows" } else { (Get-Content /etc/os-release -EA SilentlyContinue | Where-Object { $_ -match '^PRETTY_NAME=' } | Select-Object -First 1) -replace 'PRETTY_NAME="?|"$', '' }
Write-Host ("a screen recorded on {0} round trips exactly ({1} frame(s))" -f $(if ($platform) { $platform } else { "this machine" }), $frames)

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
exit 0
