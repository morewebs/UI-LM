# Checks that the live page changes while a program is drawing.
#
# This exists because three attempts to answer the question by hand produced one false alarm and
# one check that silently did not run. It prints what it compared on every run, and it carries
# its own control: the same comparison against a finished recording must come out *equal*. A
# check that can only pass is not a check.
#
#   powershell -File scripts/check-live-page.ps1
#
# Exit code 1 if the live page does not change, if either fetch is empty, or if the control does
# not behave as a frozen page must.

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
$serve = Join-Path $repo "target\debug\ui-lm-serve.exe"
if (-not (Test-Path $capture) -or -not (Test-Path $serve)) { throw "build first: cargo build --workspace" }

$port = 8891
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-livepage-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$session = Join-Path $work "live"
$control = Join-Path $work "keys.txt"
Set-Content -LiteralPath $control -Value "" -Encoding ASCII
$failures = @()

# A program that keeps drawing for twenty seconds, so the page has every reason to change.
$recorder = Start-Process -FilePath $capture -PassThru -NoNewWindow -ArgumentList @(
    "record", "--rows", "20", "--cols", "80", "--out", $session, "--control", $control,
    "--stop-after", "24000", "--cwd", $work, "--", "powershell", "-NoProfile", "-Command",
    "1..40 | ForEach-Object { 'tick ' + `$_ ; Start-Sleep -Milliseconds 300 }")
Start-Sleep -Seconds 6
$viewer = Start-Process -FilePath $serve -PassThru -NoNewWindow -ArgumentList @(
    "--session", $session, "--keys", $control, "--port", "$port")
Start-Sleep -Seconds 2

function Fetch($uri) {
    try {
        return [string](Invoke-WebRequest -Uri $uri -UseBasicParsing -TimeoutSec 5).Content
    } catch {
        return ""
    }
}

$first = Fetch "http://127.0.0.1:$port/"
Start-Sleep -Seconds 3
$second = Fetch "http://127.0.0.1:$port/"
# Printed unconditionally: a check that says nothing did nothing.
Write-Host ("live page: {0} bytes, then {1} bytes" -f $first.Length, $second.Length)

if (-not $first -or -not $second) {
    $failures += "a fetch came back empty"
} elseif ($first -eq $second) {
    $failures += "the page did not change across three seconds of a drawing program"
} elseif ($first -notmatch "tick") {
    $failures += "the page did not contain the program's output"
} else {
    Write-Host "the live page changes while the program draws"
}

if (-not $viewer.HasExited) { $viewer.Kill() }

# The control: the same two fetches against a finished recording must be identical. If they are
# not, this instrument is measuring something other than "did the page change".
if (-not $recorder.HasExited) { Start-Sleep -Seconds 12 }
if (-not $recorder.HasExited) { $recorder.Kill() }
$controlViewer = Start-Process -FilePath $serve -PassThru -NoNewWindow -ArgumentList @(
    "--session", $session, "--keys", $control, "--port", "$($port + 1)")
Start-Sleep -Seconds 3
$frozenFirst = Fetch "http://127.0.0.1:$($port + 1)/"
Start-Sleep -Seconds 2
$frozenSecond = Fetch "http://127.0.0.1:$($port + 1)/"
Write-Host ("finished recording: {0} bytes, then {1} bytes" -f $frozenFirst.Length, $frozenSecond.Length)
if (-not $frozenFirst -or -not $frozenSecond) {
    $failures += "the control fetch came back empty"
} elseif ($frozenFirst -ne $frozenSecond) {
    $failures += "a finished recording kept changing, so this check cannot tell the two cases apart"
} else {
    Write-Host "a finished recording holds still, as it must"
}
if (-not $controlViewer.HasExited) { $controlViewer.Kill() }

# The frozen document has to hold what the recording holds. Size and stability are about the *viewer*; this is about
# the viewer drawing the page rather than a placeholder that happens to change.
#
# The first version of this reported "only 1 word available": the program prints `tick 22`, so `tick` is the only word
# of four or more characters, and the filter was measuring its own threshold. Lines are the right unit here.
$screenLines = @(& $capture tui --session $session 2>$null | Where-Object { $_.Trim() })
$visible = $frozenFirst -replace '<[^>]+>', '' -replace '&#39;', "'" -replace '&quot;', '"' -replace '&lt;', '<' -replace '&gt;', '>' -replace '&amp;', '&'
$absent = @()
$checked = 0
foreach ($line in $screenLines) {
    foreach ($word in ($line -split '\s+')) {
        $word = $word.Trim()
        if ($word.Length -lt 3) { continue }
        $checked++
        if ($visible -notmatch [regex]::Escape($word)) { $absent += $word }
    }
}
if ($screenLines.Count -eq 0) {
    $failures += "the recording held no screen to compare the document against"
} elseif ($checked -lt 5) {
    $failures += ("only " + $checked + " word(s) were available to compare, so the document was not really checked")
} elseif ($absent.Count -gt 0) {
    $failures += ("the served document is missing " + $absent.Count + " word(s), such as " + (($absent | Select-Object -Unique | Select-Object -First 3) -join ", "))
} else {
    Write-Host ("the served document holds the recording's own text: {0} word(s) checked across {1} line(s)" -f $checked, $screenLines.Count)
}

# The viewer can serve the **model's** pages (`--model`), and nothing tested that path: `check-html` draws model pages
# to a file and compares them, but the live viewer had only ever been asked for the reader's. The recording is already
# finished, so this costs one more viewer rather than one more capture.
$modelPath = Join-Path $repo "models\page.json"
if ((Test-Path $modelPath) -and (Test-Path $serve)) {
    $modelViewer = Start-Process -FilePath $serve -PassThru -NoNewWindow -ArgumentList @(
        "--session", $session, "--keys", $control, "--port", "$($port + 2)", "--model", $modelPath)
    Start-Sleep -Seconds 3
    $modelPage = Fetch "http://127.0.0.1:$($port + 2)/"
    if (-not $modelViewer.HasExited) { $modelViewer.Kill() }
    if (-not $modelPage) {
        $failures += "the viewer serving the model's pages came back empty"
    } else {
        $modelVisible = $modelPage -replace '<[^>]+>', ''
        $modelAbsent = @()
        foreach ($line in $screenLines) {
            foreach ($word in ($line -split '\s+')) {
                $word = $word.Trim()
                if ($word.Length -lt 3) { continue }
                if ($modelVisible -notmatch [regex]::Escape($word)) { $modelAbsent += $word }
            }
        }
        if ($modelAbsent.Count -gt 0) {
            $failures += ("the model's served document is missing " + $modelAbsent.Count + " word(s), such as " + (($modelAbsent | Select-Object -Unique | Select-Object -First 3) -join ", "))
        } else {
            Write-Host ("the model's page is served live too: {0} bytes, every word present" -f $modelPage.Length)
        }
    }
}

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "the live page moves and a finished one does not"
