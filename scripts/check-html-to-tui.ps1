# GUI -> TUI: the document must hold enough to rebuild the screen it came from.
#
# The first direction is checked everywhere; this file checks the second: take only the rendered HTML, pull the text back
# out of it in document order, and compare with what the terminal rendering gives for the same frame.
#
# Every probed session rebuilds completely. `Meter` is the one kind that cannot: it is drawn as a bar with a track and a
# fill, and its *text* is not in a line element at all. That is by design and is measured by `check-kinds`, which
# proves the bar reaches the markup. An earlier round wrongly included `Frame` in this limitation: its rows are line

. (Join-Path $PSScriptRoot "utf8.ps1")
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { $capture = Join-Path $repo "target\release\ui-lm-capture.exe" }
if (-not (Test-Path $capture)) { Write-Host "no capture binary, so nothing was measured"; exit 1 }

$probes = @(
    @{ Session = "eval\structured\diff-f1"; Seq = 0; Why = "a diff page" },
    @{ Session = "eval\apps\pwsh-table";    Seq = 1; Why = "a table page" },
    @{ Session = "cases\hard\whiptail-dialog";   Seq = 0; Why = "a floating window" },
    @{ Session = "cases\hard\pwsh-word-table";  Seq = 1; Why = "a table of words" },
    @{ Session = "eval\apps\git-help";          Seq = 0; Why = "a help listing" },
    @{ Session = "cases\hard\pane-with-tables";  Seq = 1; Why = "two panes side by side" },
    @{ Session = "eval\apps\git-diff";         Seq = 0; Why = "a diff from git itself" },
    @{ Session = "eval\apps\git-clone";        Seq = 1; Why = "a limit from git itself" },
    @{ Session = "eval\freebuff\second-agent-18"; Seq = 1; Why = "calls and thoughts" }
)

function Get-LinesFromHtml {
    param([string]$Html)
    $lines = @()
    foreach ($match in [regex]::Matches($Html, '<div class="line[^>]*>(.*?)</div>')) {
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
        # The reader the product ships, not a copy of it: `Get-LinesFromHtml` was this check's own regex and could
    # agree with the renderer while `ui-lm-capture read` was broken (round 63).
    $previousRead = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $readRaw = & $capture read --html $out 2>&1
    $ErrorActionPreference = $previousRead
    $fromHtml = @($readRaw | ForEach-Object { "$_" } | Where-Object { $_ -ne "" })
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

# A document this project did not write, which is the handoff's "GUI that was not built here": a plain table with no
# classes, whose columns must arrive padded exactly as a terminal would draw them (round 63).
$foreignPath = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-foreign-" + [guid]::NewGuid().ToString("N") + ".html")
Set-Content -LiteralPath $foreignPath -Encoding UTF8 -Value @"
<table>
<tr><th>Name</th><th>Mode</th></tr>
<tr><td>hosts</td><td>-a----</td></tr>
<tr><td>lmhosts.sam</td><td>-a----</td></tr>
</table>
"@
$previousRead = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$foreignLines = @(& $capture read --html $foreignPath 2>&1 | ForEach-Object { "$_" })
$ErrorActionPreference = $previousRead
Remove-Item -LiteralPath $foreignPath -Force -ErrorAction SilentlyContinue
if ($foreignLines.Count -lt 3) {
    $failures += "a table this project did not write came back with no rows"
} elseif ($foreignLines[0].Trim() -ne "Name         Mode") {
    $failures += ("a foreign table's columns did not line up: " + $foreignLines[0])
} else {
    Write-Host ("a foreign table reads back as {0} line(s): {1}" -f $foreignLines.Count, $foreignLines[1].Trim())

# Two more shapes a page from anywhere can have: a form, and a wrapper `<div>` that holds blocks. Both were found by
# feeding the reader documents it had never seen, and both must keep working (rounds 68-80).
$shapePath = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-shape-" + [guid]::NewGuid().ToString("N") + ".html")
Set-Content -LiteralPath $shapePath -Encoding UTF8 -Value @"
<body><div><h1>Sign up</h1><p>Fill this in.</p>
<form><label>Name</label><input value="Jane"><button>Save</button></form></div></body>
"@
$previousRead = $ErrorActionPreference
$ErrorActionPreference = "Continue"
$shapeLines = @(& $capture read --html $shapePath 2>&1 | ForEach-Object { "$_" })
$ErrorActionPreference = $previousRead
Remove-Item -LiteralPath $shapePath -Force -ErrorAction SilentlyContinue
$want = @("Sign up", "=======", "Fill this in.", "Name", "[ Jane ]", "[ Save ]")
if ($shapeLines.Count -ne $want.Count) {
    $failures += ("a document with a wrapper div read back as {0} line(s), not {1}: {2}" -f $shapeLines.Count, $want.Count, ($shapeLines -join " | "))
} elseif (($shapeLines -join "`n") -ne ($want -join "`n")) {
    $failures += ("a wrapper div or a form did not read as a screen: " + ($shapeLines -join " | "))
} else {
    Write-Host ("a wrapper div and a form read back as {0} line(s), in order" -f $shapeLines.Count)

# Every row of the catalogue's "A page may arrive from anywhere" table, read one document at a time (round 91).
$contractShapes = @(
    @{ What = "a table";      Html = "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>"; Want = @("A  B", "1  2") },
    @{ What = "a code block"; Html = "<pre>line one`n    line two</pre>";                                   Want = @("line one", "    line two") },
    @{ What = "a heading";    Html = "<h3>Part</h3>";                                                       Want = @("Part") },
    @{ What = "a list";       Html = "<ul><li>a</li></ul>";                                                 Want = @("- a") },
    @{ What = "a definition"; Html = "<dl><dt>T</dt><dd>D.</dd></dl>";                                      Want = @("T", "    D.") },
    @{ What = "a form";       Html = '<form><label>L</label><input value="V"><button>B</button></form>';     Want = @("L", "[ V ]", "[ B ]") },
    @{ What = "an image";     Html = '<p>see <img alt="pic"> here</p>';                                     Want = @("see pic here") },
    @{ What = "a break";      Html = "<p>a<br>b</p>";                                                       Want = @("a", "b") },
    @{ What = "a summary";    Html = "<details><summary>S</summary><p>H.</p></details>";                    Want = @("S", "H.") },
    # A page built from `div` and CSS: two spans that a stylesheet puts side by side read as two cells, one space apart,
    # because the reader cannot recover a column width from a class name (round 92).
    @{ What = "two CSS cells"; Html = '<div class="row"><span class="k">Key</span><span class="v">Value</span></div>'; Want = @("Key Value") }
    # A page built the way a framework builds one: landmarks wrapping content, text inside spans. Every piece of text must
    # appear exactly once - the header's own beside its nav's (rounds 126-129).
    @{ What = "a landmark page";
       Html = "<header><span>Acme</span><nav><span>Docs</span></nav></header><main><h2>Ship</h2></main><footer><span>(c) 2026</span></footer>";
       Want = @("Acme", "Docs", "Ship", "----", "(c) 2026") }
    # What a real page carries that nobody sees: code, styles and templates. Only the content may reach a screen (rounds 132-133).
    @{ What = "a page with code";
       Html = "<style>p{color:red}</style><script>var x=1;</script><template><p>Hidden.</p></template><p>Real text.</p>";
       Want = @("Real text.") }
)
foreach ($shape in $contractShapes) {
    $one = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-shape-" + [guid]::NewGuid().ToString("N") + ".html")
    Set-Content -LiteralPath $one -Value $shape.Html -Encoding UTF8
    $previousRead = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $got = @(& $capture read --html $one 2>&1 | ForEach-Object { "$_" })
    $ErrorActionPreference = $previousRead
    Remove-Item -LiteralPath $one -Force -ErrorAction SilentlyContinue
    if (($got -join "`n") -ne ($shape.Want -join "`n")) {
        $failures += ("the catalogue promises {0} reads as a shape, and it reads as: {1}" -f $shape.What, ($got -join " | "))
    }
}
if ($failures.Count -eq 0) { Write-Host ("every shape the catalogue names reads back: {0}" -f $contractShapes.Count) }
}
}

} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

# Every kind must appear in at least one round-tripped session. This is judged **before** the verdict: written after it, the
# assertion only ran when the run was already healthy and was skipped exactly when something else had failed (rounds 117-120).
$covered = @{}
foreach ($probe in $probes) {
    $path = Join-Path $repo $probe.Session
    if (-not (Test-Path $path)) { continue }
    foreach ($line in (& $capture page --session $path --all 2>$null)) {
        foreach ($m in [regex]::Matches("$line", '"component":"(\w+)"')) { $covered[$m.Groups[1].Value] = $true }
    }
}
$missingKinds = @("Block", "ToolCall", "Thought", "Meter", "Diff", "Frame", "Table" | Where-Object { -not $covered.ContainsKey($_) })
if ($missingKinds.Count -gt 0) {
    $failures += ("the round trip covers no session holding: " + ($missingKinds -join ", "))
}

if ($checked -lt $probes.Count) {
    Write-Host ("only " + $checked + " of " + $probes.Count + " session(s) produced both renderings, so the round trip was not really checked")
}
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a document cannot be turned back into the screen it came from"
    exit 1
}
Write-Host ("the document can be turned back into the screen: {0} session(s), {1} line(s) rebuilt" -f $checked, $compared)
Write-Host ("every kind appears in a round-tripped session: {0}" -f $covered.Count)
exit 0
