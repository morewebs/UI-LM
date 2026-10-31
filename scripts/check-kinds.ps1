# The renderer must carry the **kind** of every component into the document, not only its words.
#
# `check-html` compares words and panes; a renderer that dropped the class for, say, a diff would pass every word
# comparison in this repository while producing a page a person could not tell from plain text. This check asks the
# other question: does the markup name the kinds the page holds?
#
# It is a separate file because an earlier attempt to graft it into `check-html` passed a mutation it should have
# caught - the block never ran, and an unproven check is worse than none (round 235).

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
