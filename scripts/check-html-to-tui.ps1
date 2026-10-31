# GUI -> TUI: the document must hold enough to rebuild the screen it came from.
#
# The first direction is checked everywhere; this file checks the second: take only the rendered HTML, pull the text back
# out of it in document order, and compare with what the terminal rendering gives for the same frame.
#
# Every probed session rebuilds completely. `Meter` is the one kind that cannot: it is drawn as a bar with a track and a
# fill, and its *text* is not in a line element at all. That is by design and is measured by `check-kinds`, which
# proves the bar reaches the markup. An earlier round wrongly included `Frame` in this limitation: its rows are line

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { $capture = Join-Path $repo "target\release\ui-lm-capture.exe" }
if (-not (Test-Path $capture)) { Write-Host "no capture binary, so nothing was measured"; exit 1 }

$probes = @(
    @{ Session = "eval\structured\diff-f1"; Seq = 0; Why = "a diff page" },
    @{ Session = "eval\apps\pwsh-table";    Seq = 1; Why = "a table page" },
    @{ Session = "cases\hard\whiptail-dialog";   Seq = 0; Why = "a floating window" }
)

function Get-LinesFromHtml {
    param([string]$Html)
    $lines = @()
    foreach ($match in [regex]::Matches($Html, '<div class="line[^"]*">(.*?)</div>')) {
        $text = [regex]::Replace($match.Groups[1].Value, '<[^>]+>', '')
        $text = $text.Replace('&lt;', '<').Replace('&gt;', '>').Replace('&quot;', '"').Replace('&#39;', "'").Replace('&amp;', '&')
        $lines += $text.TrimEnd()
    }
    return $lines
}

$failures = @(); $checked = 0; $compared = 0
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-html2tui-" + [guid]::NewGuid().ToString("N"))

try {
    New-Item -ItemType Directory -Force -Path $work | Out-Null
    foreach ($probe in $probes) {
        $session = Join-Path $repo $probe.Session
        if (-not (Test-Path $session)) { $failures += ("no such session: " + $probe.Session); continue }
        $out = Join-Path $work (([guid]::NewGuid().ToString("N")) + ".html")
        & $capture render --session $session --all --out $out 2>&1 | Out-Null
        if (-not (Test-Path $out)) { $failures += ("the renderer wrote nothing for " + $probe.Session); continue }
        $fromHtml = Get-LinesFromHtml (Get-Content -LiteralPath $out -Raw -Encoding UTF8)
        $previous = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        $raw = & $capture tui --session $session --seq $probe.Seq 2>&1
        $ErrorActionPreference = $previous
        $fromTui = @($raw | ForEach-Object { "$_" } | Where-Object { $_.Trim() -and $_ -notmatch 'round trip:' })
        if ($fromHtml.Count -eq 0) { $failures += ("the document held no lines: " + $probe.Session); continue }
        if ($fromTui.Count -eq 0) { $failures += ("the terminal rendering held no lines: " + $probe.Session); continue }
        $checked++
        $missing = 0
        foreach ($line in $fromTui) {
            $want = $line.TrimEnd()
            if (-not $want) { continue }
            $compared++
            if ($fromHtml -notcontains $want) { $missing++ }
        }
        if ($missing -gt 0) { $failures += ("{0} ({1}): {2} line(s) of the screen cannot be rebuilt from the document" -f $probe.Session, $probe.Why, $missing) }
    }
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

if ($checked -lt $probes.Count) { Write-Host ("only " + $checked + " of " + $probes.Count + " session(s) produced both renderings, so the round trip was not really checked"); exit 1 }
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a document cannot be turned back into the screen it came from"
    exit 1
}
Write-Host ("the document can be turned back into the screen: {0} session(s), {1} line(s) rebuilt" -f $checked, $compared)
exit 0
