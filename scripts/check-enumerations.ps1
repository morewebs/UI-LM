# A list of kinds that does not hold every kind is a place where a new one goes missing.
#
# Adding `Table` exposed the same defect four times: the ledger lacked a check, the runner lacked an entry, the markup
# probes named six, and `check-fresh`'s per-kind report filtered for five. Every one of those lists was correct when it
# was written, and none of them was updated when the vocabulary grew.
#
# This check reads the catalogue and holds every enumeration it can find to it: a run of three or more kind names joined
# by `|` (a regex alternation) must name every kind the catalogue defines. Lists of one or two names are ignored, because
# a script may legitimately care about `Table` and `Frame` alone.

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot

$source = Get-Content -LiteralPath (Join-Path $repo "crates\pages\src\lib.rs") -Raw -Encoding UTF8
$match = [regex]::Match($source, 'pub const COMPONENTS: \[&str; (\d+)\] = \[([\s\S]*?)\]')
if (-not $match.Success) {
    Write-Host "the catalogue constant could not be read, so nothing was measured"
    exit 1
}
$kinds = @([regex]::Matches($match.Groups[2].Value, '"([A-Za-z]+)"') | ForEach-Object { $_.Groups[1].Value } |
    Where-Object { $_ -ne "Column" })
if ($kinds.Count -lt 3) {
    Write-Host "the catalogue holds too few kinds to check an enumeration against"
    exit 1
}

$files = @()
$files += Get-ChildItem -LiteralPath (Join-Path $repo "scripts") -Filter "*.ps1" | ForEach-Object { $_.FullName }
$files += Get-ChildItem -LiteralPath (Join-Path $repo "docs") -Filter "*.md" -Recurse | ForEach-Object { $_.FullName }
$files += (Join-Path $repo "README.md")
$files += (Join-Path $repo "HANDOFF.md")

$failures = @()
$checked = 0

foreach ($file in $files) {
    if (-not (Test-Path $file)) { continue }
    $text = Get-Content -LiteralPath $file -Raw -Encoding UTF8
    # A regex alternation of kind names, as written in a filter or a pattern.
    foreach ($m in [regex]::Matches($text, '\b((?:Block|ToolCall|Meter|Diff|Thought|Frame|Table)(?:\|(?:Block|ToolCall|Meter|Diff|Thought|Frame|Table))+)\b')) {
        $checked++
        $named = @($m.Groups[1].Value -split '\|' | Select-Object -Unique)
        $missing = @($kinds | Where-Object { $named -notcontains $_ })
        if ($missing.Count -gt 0) {
            $relative = $file.Substring($repo.Length).TrimStart('\')
            $failures += ("{0}: lists {1} kind(s) and leaves out {2}" -f $relative, $named.Count, ($missing -join ", "))
        }
    }
}

if ($checked -lt 2) {
    Write-Host ("only {0} enumeration(s) were found, so the vocabulary lists were not really checked" -f $checked)
    exit 1
}
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a list of kinds does not hold every kind the catalogue defines"
    exit 1
}
Write-Host ("every enumeration names all {0} kinds: {1} list(s) checked" -f $kinds.Count, $checked)
exit 0
