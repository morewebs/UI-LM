# The renderer must carry the **kind** of every component into the document, not only its words.
#
# `check-html` compares words and panes; a renderer that dropped the class for, say, a diff would pass every word
# comparison in this repository while producing a page a person could not tell from plain text. This check asks the
# other question: does the markup name the kinds the page holds?
#
# It is a separate file because an earlier attempt to graft it into `check-html` passed a mutation it should have
# caught - the block never ran, and an unproven check is worse than none (round 235).

. (Join-Path $PSScriptRoot "utf8.ps1")
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { $capture = Join-Path $repo "target\release\ui-lm-capture.exe" }
if (-not (Test-Path $capture)) {
    Write-Host "no capture binary, so nothing was measured"
    exit 1
}

# Each probe is a session and a class its document must contain. The class is what the renderer emits today, verified
# by reading the markup - not what it ought to emit.
$probes = @(
    @{ Session = "eval\structured\diff-f1";          Class = "block diff";  Why = "a diff must not read as a block of text" },
    @{ Session = "eval\freebuff\multi-file-task";    Class = "thought";     Why = "an agent's reasoning must not read as a block" },
    @{ Session = "eval\freebuff\multi-file-task";    Class = "tool";        Why = "a tool call must not read as a block" },
    @{ Session = "cases\hard\whiptail-dialog";       Class = "component frame"; Why = "a floating window must not read as a block" },
    # A meter must arrive as a drawn bar, not as the words `3/5`: the kind exists so a GUI can show a limit, and a page
    # that only carries the text has thrown away the only thing the kind was for.
    @{ Session = "eval\nongrok\banner";              Class = "meter";         Why = "a budget must not read as a block of text" },
    @{ Session = "eval\nongrok\banner";              Class = "fill";          Why = "a meter must be drawn, not described" },
    @{ Session = "eval\tui\df-table";                Class = "block table";   Why = "a table must not read as a block of text" }
)

$failures = @()
$checked = 0
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-kinds-" + [guid]::NewGuid().ToString("N"))

try {
    New-Item -ItemType Directory -Force -Path $work | Out-Null
    foreach ($probe in $probes) {
        $session = Join-Path $repo $probe.Session
        if (-not (Test-Path $session)) {
            $failures += ("no such session: " + $probe.Session)
            continue
        }
        $out = Join-Path $work (([guid]::NewGuid().ToString("N")) + ".html")
        & $capture render --session $session --all --out $out 2>&1 | Out-Null
        if (-not (Test-Path $out)) {
            $failures += ("the renderer wrote nothing for " + $probe.Session)
            continue
        }
        $html = Get-Content -LiteralPath $out -Raw
        $checked++
        if ($html -notmatch ('class="' + [regex]::Escape($probe.Class) + '"')) {
            $failures += ("{0}: its document holds no {1} - {2}" -f $probe.Session, $probe.Class, $probe.Why)
        }
    }
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

# A shortcut must arrive as a **button carrying its binding**, which is what the page model promises and what the
# renderer's own test asserts. `git help`'s list of file formats had been read as shortcuts, so the rule behind them was
# narrowed before this probe could exist (rounds 30-32).
$shortcutSession = Join-Path $repo "eval\unseen\unseen-listing"
if (Test-Path $shortcutSession) {
    $htmlPath = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-shortcuts-" + [guid]::NewGuid().ToString("N") + ".html")
    $previous = $ErrorActionPreference
    $raw = & $capture render --session $shortcutSession --all --out $htmlPath 2>&1
    $ErrorActionPreference = $previous
    $html = if (Test-Path $htmlPath) { Get-Content -LiteralPath $htmlPath -Raw -Encoding UTF8 } else { "" }
    Remove-Item -LiteralPath $htmlPath -Force -ErrorAction SilentlyContinue
    $buttons = [regex]::Matches($html, '<button class="action" data-binding="[^"]+">[^<]+</button>').Count
    if ($buttons -lt 1) {
        $failures += "the document holds no shortcut button, and the page holds shortcuts"
    } else {
        Write-Host ("shortcut buttons reach the document: {0}" -f $buttons)
    }
}

# A property must arrive as a **field**, not as a tool call: the renderer drew `Author: …` with `class="tool"`, so no
# consumer could tell a page's metadata from a `Read` action (round 39).
$propertySession = Join-Path $repo "eval\apps\git-logstat"
if (Test-Path $propertySession) {
    $fieldPath = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-props-" + [guid]::NewGuid().ToString("N") + ".html")
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & $capture render --session $propertySession --all --out $fieldPath 2>&1 | Out-Null
    $ErrorActionPreference = $previous
    $fieldHtml = if (Test-Path $fieldPath) { Get-Content -LiteralPath $fieldPath -Raw -Encoding UTF8 } else { "" }
    Remove-Item -LiteralPath $fieldPath -Force -ErrorAction SilentlyContinue
    $fields = [regex]::Matches($fieldHtml, '<div class="property"><span class="name">[^<]+</span>').Count
    if ($fields -lt 1) {
        $failures += "the document holds no property field, and the page holds properties"
    } else {
        Write-Host ("property fields reach the document: {0}" -f $fields)
    }
}

# The remaining rows of the catalogue's contract, each on a session that holds that part: a tab strip, a table's cells,
# and panes laid out side by side. Round 47 walked these by hand; this keeps the walk (round 48).
$contractRows = @(
    @{ Session = "eval\tui\wsl-vim-numbered-tabs"; Pattern = 'class="tabs"';        What = "a tab strip" },
    @{ Session = "eval\tui\df-table";              Pattern = 'data-cells="';        What = "a table's cells" },
    @{ Session = "cases\hard\pane-with-tables";    Pattern = 'data-layout="row"';   What = "panes side by side" }
)
foreach ($row in $contractRows) {
    $path = Join-Path $repo $row.Session
    if (-not (Test-Path $path)) { continue }
    $out = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-row-" + [guid]::NewGuid().ToString("N") + ".html")
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    & $capture render --session $path --all --out $out 2>&1 | Out-Null
    $ErrorActionPreference = $previous
    $html = if (Test-Path $out) { Get-Content -LiteralPath $out -Raw -Encoding UTF8 } else { "" }
    Remove-Item -LiteralPath $out -Force -ErrorAction SilentlyContinue
    if (-not $html.Contains($row.Pattern)) {
        $failures += ("the document does not carry {0}: no {1} in {2}" -f $row.What, $row.Pattern, $row.Session)
    } else {
        Write-Host ("the contract holds: {0}" -f $row.What)
    }
}

# The model's pages must carry the kinds too. `page --all --model` served a `Block` where the reader served a `Table`
# for the same screen, and every check here passed because none of them looked at a model page (round 19).
$modelFile = Join-Path $repo "models\page.json"
if (Test-Path $modelFile) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $raw = & $capture page --session (Join-Path $repo "eval\tui\df-table") --all --model $modelFile 2>&1
    $ErrorActionPreference = $previous
    $kinds = @()
    foreach ($line in @($raw | ForEach-Object { "$_" })) {
        if (-not $line.TrimStart().StartsWith("{")) { continue }
        $page = $line | ConvertFrom-Json
        $kinds += @($page.createSurface.components | ForEach-Object { $_.component })
    }
    if ($kinds -notcontains "Table") {
        $failures += "the model's pages hold no Table for a table screen, and the reader's do"
    } else {
        Write-Host "the model's pages carry the shaping rules too: a Table for a table screen"
    }
}

if ($checked -lt $probes.Count) {
    Write-Host ("only " + $checked + " of " + $probes.Count + " documents were rendered, so the renderer was not really checked")
    exit 1
}
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a document does not carry the kind of every component"
    exit 1
}
Write-Host ("every document carries its kinds: " + $probes.Count + " probe(s), " + $checked + " document(s)")
exit 0
