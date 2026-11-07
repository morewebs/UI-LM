# Documents entrench what the code has left behind.
#
# Adding the seventh kind exposed the same defect four times over: the README's heading said seven and its list held six,
# the README and the handoff still called `Table` "rejected" while it shipped, and the model card's boundary table said
# `Table` "does not exist". Every one of those sentences was true when written and none of them were rewritten.
#
# This check reads the catalogue constant and holds two claims to it:
#   1. a document that states how many kinds exist must state the number the code has;
#   2. a document may not call a component "rejected" or say it "does not exist" while the catalogue defines it.

. (Join-Path $PSScriptRoot "utf8.ps1")
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot

$source = Get-Content -LiteralPath (Join-Path $repo "crates\pages\src\lib.rs") -Raw -Encoding UTF8
$match = [regex]::Match($source, 'pub const COMPONENTS: \[&str; (\d+)\] = \[([\s\S]*?)\]')
if (-not $match.Success) {
    Write-Host "the catalogue constant could not be read, so nothing was measured"
    exit 1
}
$declared = [int]$match.Groups[1].Value
$names = @([regex]::Matches($match.Groups[2].Value, '"([A-Za-z]+)"') | ForEach-Object { $_.Groups[1].Value })
# `Column` is the root every page has; the kinds a document counts are the components a screen can hold.
$kinds = @($names | Where-Object { $_ -ne "Column" })

if ($names.Count -ne $declared) {
    Write-Host ("the constant says {0} and lists {1}" -f $declared, $names.Count)
    exit 1
}
if ($kinds.Count -lt 2) {
    Write-Host "the catalogue holds too few kinds to check anything"
    exit 1
}

$documents = @("README.md", "HANDOFF.md", "GLOSSARY.md", "docs\model-card.md",
               "docs\reports\2026-10-28-state-of-the-project.md", "docs\reports\2026-10-29-month-in-review.md")
$failures = @()
$checked = 0

foreach ($relative in $documents) {
    $path = Join-Path $repo $relative
    if (-not (Test-Path $path)) { continue }
    $text = Get-Content -LiteralPath $path -Raw -Encoding UTF8
    $checked++

    # 1. a stated count of kinds. A sentence about what the **model labels** is a different claim: the model labels
    # five of the seven, and that number is checked by the model card's own consistency, not here.
    foreach ($line in ($text -split "`n")) {
        # Only sentences about the catalogue's **whole** vocabulary are counted. "Those two kinds are drawn by
        # rules" is true and says nothing about how many exist (round 25).
        if ($line -match 'label') { continue }
        # Table rows count too: the handoff's evidence row carried "seven kinds" over six names, and the filter skipped
        # it because it starts with `|` - the sentence the guard exists for was the one it could not see.
        if ($line -notmatch 'catalogue|vocabulary|holds \w+ kinds|^(#|\s*[-*]\s|\|)') { continue }
        foreach ($m in [regex]::Matches($line, '(\w+)\s+kinds\b')) {
            $word = $m.Groups[1].Value.ToLower()
            $digits = @{ "one"=1; "two"=2; "three"=3; "four"=4; "five"=5; "six"=6; "seven"=7; "eight"=8; "nine"=9 }
            $stated = if ($digits.ContainsKey($word)) { $digits[$word] } elseif ($word -match '^\d+$') { [int]$word } else { -1 }
            if ($stated -gt 0 -and $stated -ne $kinds.Count) {
                $failures += ("{0}: says {1} kinds; the catalogue defines {2}" -f $relative, $stated, $kinds.Count)
            }
            # A sentence that states the count and lists the kinds must list them all. The handoff's evidence row said
            # "seven kinds" over six names for as long as Table existed; counting the sentence was not reading it.
            $prose = @{ "Block"='block'; "ToolCall"='tool call'; "Meter"='meter'; "Thought"='thought|reasoning';
                        "Diff"='diff'; "Frame"='frame'; "Table"='table' }
            $named = @($kinds | Where-Object { $line -match $prose[$_] })
            if ($named.Count -ge 3) {
                $left = @($kinds | Where-Object { $named -notcontains $_ })
                if ($left.Count -gt 0) {
                    $failures += ("{0}: states the count and its list leaves out {1}" -f $relative, ($left -join ", "))
                }
            }
        }
    }

    # 2. a component called rejected or absent
    foreach ($kind in $kinds) {
        foreach ($pattern in @("$kind``? is rejected", "$kind``? does not exist", "$kind``? was rejected")) {
            if ($text -match $pattern) {
                $failures += ("{0}: calls {1} rejected or absent, and the catalogue defines it" -f $relative, $kind)
            }
        }
    }
}

# The catalogue's draw contract says what a runtime owes each part of a page; a kind missing from that table is a kind
# the guide never tells a stranger to draw. `check-kinds` proves the renderer draws all seven; this proves the table says so.
$catalogue = Join-Path $repo "docs\catalogue\moreweb.terminal-v1.md"
if (Test-Path $catalogue) {
    $text = Get-Content -LiteralPath $catalogue -Raw -Encoding UTF8
    $section = [regex]::Match($text, '## What a runtime must draw([\s\S]*?)(
## |\z)')
    if (-not $section.Success) {
        Write-Host "the catalogue no longer holds the draw contract, so nothing was compared"
        exit 1
    }
    # Only the table's rows count. The prose after it mentions `Table` and `Frame` - "whether a `Frame` is a dialog
    # element" - and a first version that read the whole section passed a mutation that removed the row.
    $rows = (($section.Groups[1].Value -split "`n") | Where-Object { $_ -match '^\|' }) -join "`n"
    $unlisted = @($kinds | Where-Object { $rows -notmatch ('`' + [regex]::Escape($_) + '`') })
    if ($unlisted.Count -gt 0) {
        Write-Host ("  the draw contract does not list: " + ($unlisted -join ", "))
        Write-Host "the catalogue does not tell a runtime to draw every kind"
        exit 1
    }
    Write-Host ("the draw contract names every kind: {0}" -f $kinds.Count)
}

# A glossary that says "seven kinds" and defines six matches every count check and still leaves one kind a stranger cannot
# look up: `Table` was missing for as long as the kind existed. The section must define each kind with a bold entry.
$glossary = Join-Path $repo "GLOSSARY.md"
if (Test-Path $glossary) {
    $text = Get-Content -LiteralPath $glossary -Raw -Encoding UTF8
    $section = [regex]::Match($text, '## The \w+ kinds([\s\S]*?)(
## |\z)')
    if (-not $section.Success) {
        Write-Host "the glossary no longer holds a kinds section, so nothing was compared"
        exit 1
    }
    $undefined = @($kinds | Where-Object { $section.Groups[1].Value -notmatch ('\*\*' + [regex]::Escape($_) + '\*\*') })
    if ($undefined.Count -gt 0) {
        Write-Host ("  the glossary's kinds section does not define: " + ($undefined -join ", "))
        Write-Host "the glossary counts the kinds without defining them all"
        exit 1
    }
    Write-Host ("the glossary defines every kind: {0}" -f $kinds.Count)
}

if ($checked -lt 3) {
    Write-Host ("only " + $checked + " document(s) were read, so the claims were not really checked")
    exit 1
}
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a document claims something the catalogue contradicts"
    exit 1
}
Write-Host ("every document agrees with the catalogue: {0} kind(s), {1} document(s)" -f $kinds.Count, $checked)
exit 0
