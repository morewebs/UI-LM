# The state page must name the release it is describing.
#
# A document that says "the download works" and names a version two releases old is worse than silent: it
# tells a reader what to download and is wrong. This compares the version in that row with the newest tag.

param()

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$page = Join-Path $repo "docs\reports\2026-10-28-state-of-the-project.md"
# The page claims how many kinds the vocabulary has, and the code knows: the catalogue's array minus `Column`,
# which is the container rather than a kind of thing. A claim kept in prose drifts — this one said five for as
# long as the vocabulary had six, which is the whole reason the count checks below exist.
$lib = Join-Path $repo "crates/pages/src/lib.rs"
if ((Test-Path $lib) -and (Test-Path $page)) {
    $source = Get-Content -LiteralPath $lib -Raw -Encoding UTF8
    $match = [regex]::Match($source, 'pub const COMPONENTS: \[&str; (\d+)\]')
    if ($match.Success) {
        $kinds = [int]$match.Groups[1].Value - 1
        $words = @{ 1="one"; 2="two"; 3="three"; 4="four"; 5="five"; 6="six"; 7="seven"; 8="eight" }
        $said = Get-Content -LiteralPath $page -Raw -Encoding UTF8
        $claimed = @()
        foreach ($pair in $words.GetEnumerator()) {
            if ($said -match ("all " + $pair.Value + " kinds")) { $claimed += $pair.Key }
        }
        foreach ($number in $claimed) {
            if ($number -ne $kinds) {
                Write-Host ("the page says all {0} kinds; the catalogue defines {1}" -f $words[$number], $kinds)
                exit 1
            }
        }
        if ($claimed.Count -eq 0) {
            Write-Host "the state page no longer says how many kinds there are, so nothing was compared"
            exit 1
        }
    }
}

if (-not (Test-Path $page)) { Write-Host "no state page, so nothing was compared"; exit 1 }

$tags = @(& git -C $repo tag --list "v*" | ForEach-Object { $_.Trim() } | Where-Object { $_ -match '^v\d+\.\d+\.\d+$' })
if ($tags.Count -eq 0) { Write-Host "no release tags, so there is nothing to compare against"; exit 1 }
$latest = $tags | Sort-Object { [version]($_ -replace '^v', '') } | Select-Object -Last 1

$text = Get-Content -LiteralPath $page -Raw -Encoding UTF8
$cited = $null
foreach ($line in ($text -split "`n")) {
    if ($line -match 'the download works' -and $line -match 'v(\d+\.\d+\.\d+)') { $cited = "v" + $Matches[1] }
}

Write-Host ("  the state page names: {0}; the newest tag is: {1}" -f $(if ($cited) { $cited } else { "nothing" }), $latest)
if (-not $cited) {
    Write-Host "the state page no longer says which release it describes"
    exit 1
}
# The page must not name a release *older* than the newest tag — that is how a stale front page looks. It may
# name one **newer**, because `docs/release.md` updates the documents before it tags them, so during a release the
# page is one step ahead on purpose. The old rule required an exact match, which made the documented order
# impossible: pages-first was red until the tag existed, and the only way to a green gate was to tag first.
if ([version]($cited -replace '^v', '') -lt [version]($latest -replace '^v', '')) {
    Write-Host "the state page names an older release than the newest tag"
    exit 1
}
if ($cited -ne $latest) {
    Write-Host ("  a release is in progress: the page names {0}, the newest tag is {1}" -f $cited, $latest)
}
Write-Host "the state page names the release it is describing"

# The count the documents state must be the count the scripts folder holds. This was wrong three times in
# one day — off by one while counting the runner as a check, then off again while counting rows — and each
# version read as evidence. Counting it is cheaper than believing it.
$scripts = @(Get-ChildItem -LiteralPath (Join-Path $repo "scripts") -Filter "check-*.ps1" |
    Where-Object { $_.Name -ne "check-everything.ps1" }).Count
$fuzz = @(Get-ChildItem -Path (Join-Path $repo "crates\*\tests\fuzz_*.rs")).Count
$ledgerRows = @(Select-String -LiteralPath (Join-Path $repo "docs\checks.md") -Pattern '^\| `').Count

Write-Host ("  the folder holds {0} checks and {1} fuzz properties; the ledger lists {2} script(s)" -f $scripts, $fuzz, $ledgerRows)
if ($ledgerRows -ne $scripts) {
    Write-Host "the ledger does not list every check"
    exit 1
}

# The review names the newest release too, and it must not lag: it is the page a reader opens first.
$review = Join-Path $repo "docs\reports\2026-10-29-month-in-review.md"
if (Test-Path $review) {
    $reviewText = Get-Content -LiteralPath $review -Raw -Encoding UTF8
    $named = @([regex]::Matches($reviewText, 'v0\.\d+\.\d+') | ForEach-Object { $_.Value } | Sort-Object { [version]($_ -replace '^v', '') })
    # Same rule as the state page, for the same reason: a release in progress updates the pages before it tags
    # them, so the review may be one release ahead — and must never name an older one. An exact match was the
    # rule until round 105, and it made `docs/release.md` unrunnable: pages-first was red until the tag existed.
    if ($named.Count -gt 0 -and [version]($named[-1] -replace '^v', '') -lt [version]($latest -replace '^v', '')) {
        Write-Host ("  the review's newest version is {0}; the newest tag is {1}" -f $named[-1], $latest)
        Write-Host "the review names an older release than the newest tag"
        exit 1
    }
    Write-Host ("  the review's newest version is " + $named[-1])
}

$pages = @((Join-Path $repo "docs\reports\2026-10-28-state-of-the-project.md"), (Join-Path $repo "docs\reports\2026-10-29-month-in-review.md"))
$wrong = @()
foreach ($pagePath in $pages) {
    if (-not (Test-Path $pagePath)) { continue }
    $page = Get-Content -LiteralPath $pagePath -Raw -Encoding UTF8
    foreach ($match in [regex]::Matches($page, '(\d+)\s+scripts')) {
        if ([int]$match.Groups[1].Value -ne $scripts) {
            $wrong += ("{0}: says {1} scripts, the folder holds {2}" -f (Split-Path -Leaf $pagePath), $match.Groups[1].Value, $scripts)
        }
    }
}
if ($wrong.Count -gt 0) {
    $wrong | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a document counts the checks wrongly"
    exit 1
}
Write-Host "every document counts the checks the way the folder does"

# The corpus counts the documents state, counted here. Three of them had drifted — the batches, the
# training programs and the recordings — and each read as evidence while being wrong.
$recordings = 0
$frames = 0
foreach ($root in @("eval", "cases")) {
    $dir = Join-Path $repo $root
    if (-not (Test-Path $dir)) { continue }
    foreach ($session in (Get-ChildItem -LiteralPath $dir -Recurse -Directory | Where-Object { Test-Path (Join-Path $_.FullName "session.json") })) {
        $recordings++
        $file = Join-Path $session.FullName "frames.jsonl"
        if (Test-Path $file) { $frames += @(Get-Content -LiteralPath $file | Where-Object { $_.Trim() }).Count }
    }
}
Write-Host ("  the corpus holds {0} recording(s) and {1} frame(s)" -f $recordings, $frames)
if ($recordings -lt 1 -or $frames -lt 1) {
    Write-Host "no corpus on this machine, so the counts were not checked"
    exit 1
}

# The model card states two corpus numbers of its own, and both had drifted: the evaluation sets were "185
# frames" when they were 234, and the second agent was "580" when it was 591. Neither is derivable from the
# totals above — they are two different slices of the same corpus — so each is counted here.
$card = Join-Path $repo "docs\model-card.md"
if ((Test-Path $card) -and $recordings -gt 0) {
    $evalFrames = 0
    foreach ($set in @("apps", "nongrok", "scratch", "structured", "tui", "unseen", "workflow")) {
        $dir = Join-Path $repo ("eval\" + $set)
        if (-not (Test-Path $dir)) { continue }
        foreach ($session in (Get-ChildItem -LiteralPath $dir -Recurse -Directory | Where-Object { Test-Path (Join-Path $_.FullName "session.json") })) {
            $file = Join-Path $session.FullName "frames.jsonl"
            if (Test-Path $file) { $evalFrames += @(Get-Content -LiteralPath $file | Where-Object { $_.Trim() }).Count }
        }
    }
    $agentFrames = 0
    $agentDir = Join-Path $repo "eval\freebuff"
    if (Test-Path $agentDir) {
        foreach ($session in (Get-ChildItem -LiteralPath $agentDir -Recurse -Directory | Where-Object { Test-Path (Join-Path $_.FullName "session.json") })) {
            $file = Join-Path $session.FullName "frames.jsonl"
            if (Test-Path $file) { $agentFrames += @(Get-Content -LiteralPath $file | Where-Object { $_.Trim() }).Count }
        }
    }
    $said = Get-Content -LiteralPath $card -Raw -Encoding UTF8
    $wrong = @()
    foreach ($match in [regex]::Matches($said, 'Grok and non-Grok screens, (\d+) frames')) {
        if ([int]$match.Groups[1].Value -ne $evalFrames) { $wrong += ("the card says {0} evaluation frames, the sets hold {1}" -f $match.Groups[1].Value, $evalFrames) }
    }
    foreach ($match in [regex]::Matches($said, '\*\*(\d+) recordings, (\d+) frames, never trained on\*\*')) {
        if ([int]$match.Groups[2].Value -ne $agentFrames) { $wrong += ("the card says {0} second-agent frames, the recordings hold {1}" -f $match.Groups[2].Value, $agentFrames) }
    }
    if ($wrong.Count -gt 0) {
        $wrong | ForEach-Object { Write-Host ("  " + $_) }
        Write-Host "the model card counts its material wrongly"
        exit 1
    }
    if ($evalFrames -eq 0 -or $agentFrames -eq 0) {
        Write-Host "the card's two slices are empty on this machine, so nothing was compared"
        exit 1
    }
}

$corpusPage = Join-Path $repo "docs\reports\2026-10-28-state-of-the-project.md"
if (Test-Path $corpusPage) {
    $text = Get-Content -LiteralPath $corpusPage -Raw -Encoding UTF8
    $bad = @()
    foreach ($match in [regex]::Matches($text, '(\d+)\s+recordings')) {
        if ([int]$match.Groups[1].Value -ne $recordings) { $bad += ("the page says {0} recordings, the corpus holds {1}" -f $match.Groups[1].Value, $recordings) }
    }
    foreach ($match in [regex]::Matches($text, '(\d+)\s+frames audited')) {
        if ([int]$match.Groups[1].Value -ne $frames) { $bad += ("the page says {0} frames audited, the corpus holds {1}" -f $match.Groups[1].Value, $frames) }
    }
    if ($bad.Count -gt 0) {
        $bad | ForEach-Object { Write-Host ("  " + $_) }
        Write-Host "a document counts the corpus wrongly"
        exit 1
    }
}
Write-Host "the corpus counts in the documents match the corpus"

# A document that points at a file which is not there sends its reader nowhere. Only this repository's own
# `docs/` and `scripts/` paths are checked: the reports also cite paths from other projects and from
# machines that are gone, and those are history rather than instructions.
$sources = @(Get-ChildItem -LiteralPath (Join-Path $repo "docs") -Recurse -Filter "*.md") +
    @(Get-Item -LiteralPath (Join-Path $repo "README.md"), (Join-Path $repo "HANDOFF.md") -ErrorAction SilentlyContinue)
$dead = @()
foreach ($source in $sources) {
    $text = Get-Content -LiteralPath $source.FullName -Raw -Encoding UTF8
    foreach ($match in [regex]::Matches($text, '`((?:docs|scripts)/[^`]+\.(?:md|ps1))`')) {
        $target = Join-Path $repo ($match.Groups[1].Value -replace '/', '\')
        if (-not (Test-Path $target)) {
            $dead += ("{0} points at {1}, which does not exist" -f $source.Name, $match.Groups[1].Value)
        }
    }
}
if ($dead.Count -gt 0) {
    $dead | Sort-Object -Unique | Select-Object -First 5 | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a document points at a file that is not there"
    exit 1
}
Write-Host "every docs/ and scripts/ reference in the documents resolves"
# A count written in words cannot be counted. One review said "seventeen scripts" while the folder held
# eighteen, and every digit-based check passed, so the front pages must spell their counts as digits.
$words = "sixteen|seventeen|eighteen|nineteen|twenty"
$spelled = @()
foreach ($pagePath in $pages) {
    if (-not (Test-Path $pagePath)) { continue }
    $page = Get-Content -LiteralPath $pagePath -Raw -Encoding UTF8
    foreach ($found in [regex]::Matches($page, "(?i)\b($words)[\s-]+(scripts|checks|properties)")) {
        $spelled += ("{0}: spells '{1} {2}' - counts belong in digits here" -f (Split-Path -Leaf $pagePath), $found.Groups[1].Value, $found.Groups[2].Value)
    }
}
if ($spelled.Count -gt 0) {
    $spelled | Select-Object -First 5 | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a front page spells a count that the checks cannot count"
    exit 1
}
Write-Host "every count in the front pages is written as digits"

# The hard recordings are evidence, so their number is counted too.
$hard = @(Get-ChildItem -LiteralPath (Join-Path $repo "cases\hard") -Directory -ErrorAction SilentlyContinue).Count
$corpusPage2 = Join-Path $repo "docs\reports\2026-10-28-state-of-the-project.md"
if ((Test-Path $corpusPage2) -and $hard -gt 0) {
    $text2 = Get-Content -LiteralPath $corpusPage2 -Raw -Encoding UTF8
    $bad2 = @()
    foreach ($found in [regex]::Matches($text2, '(\d+)\s+hard recording')) {
        if ([int]$found.Groups[1].Value -ne $hard) { $bad2 += ("the page says {0} hard recordings, there are {1}" -f $found.Groups[1].Value, $hard) }
    }
    if ($bad2.Count -gt 0) {
        $bad2 | ForEach-Object { Write-Host ("  " + $_) }
        Write-Host "a document counts the hard recordings wrongly"
        exit 1
    }
    Write-Host ("  the hard recordings on disk: {0}" -f $hard)
}

# The review opens by counting the releases, so that number is counted here as well.
$reviewPage = Join-Path $repo "docs\reports\2026-10-29-month-in-review.md"
if (Test-Path $reviewPage) {
    $reviewText2 = Get-Content -LiteralPath $reviewPage -Raw -Encoding UTF8
    $tagCount = @(& git -C $repo tag --list "v*" | Where-Object { $_ -match '^v\d+\.\d+\.\d+$' }).Count
    foreach ($found in [regex]::Matches($reviewText2, '(\d+)\s+releases')) {
        # The same allowance the versions get, for the same reason: a release in progress counts the release it is
        # about to make, so the review may be **one ahead** of the tags — never behind, because a page that lags
        # is a page describing the past. Exact equality was the rule until round 106.
        $said = [int]$found.Groups[1].Value
        if ($said -lt $tagCount -or $said -gt ($tagCount + 1)) {
            Write-Host ("  the review says {0} releases, there are {1}" -f $said, $tagCount)
            Write-Host "the review counts the releases wrongly"
            exit 1
        }
    }
    Write-Host ("  releases counted: {0}" -f $tagCount)
}

# A front page whose opening paragraph is a fragment is a page that lost its beginning to an edit. This
# caught a real one: the review's header was replaced by a dangling "properties. This page is the short
# version..." while a link was being added, and the page still passed every numeric check.
$opens = @{"README.md" = "open"; "HANDOFF.md" = "open"; "docs\reports\2026-10-28-state-of-the-project.md" = "open"; "docs\reports\2026-10-29-month-in-review.md" = "open"; "docs\model-card.md" = "open"; "docs\checks.md" = "open"}
$broken = @()
foreach ($docName in $opens.Keys) {
    $docPath = Join-Path $repo $docName
    if (-not (Test-Path $docPath)) { continue }
    $lines = Get-Content -LiteralPath $docPath
    $index = 0
    while ($index -lt $lines.Count -and ($lines[$index].Trim() -eq "" -or $lines[$index].StartsWith("#"))) { $index++ }
    if ($index -ge $lines.Count) { $broken += ($docName + " has no opening paragraph at all"); continue }
    $first = $lines[$index].Trim().TrimStart(">").Trim()
    $startsProperly = $first.StartsWith("**") -or $first.StartsWith("*") -or [char]::IsUpper($first[0])
    if (-not $startsProperly) {
        $broken += ("{0} opens with '{1}'" -f $docName, $first.Substring(0, [Math]::Min(50, $first.Length)))
    }
}
if ($broken.Count -gt 0) {
    $broken | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a front page opens with a fragment"
    exit 1
}
# The model card's boundary table names a guard for each rule that must not widen. A guard that is deleted from
# `check-hard` would leave the document claiming protection that no longer exists — and the document is read by
# whoever widens a rule next. So every case in the forbidden table has to be named in the card.
$hard = Join-Path $repo "scripts\check-hard.ps1"
$card = Join-Path $repo "docs\model-card.md"
if ((Test-Path $hard) -and (Test-Path $card)) {
    $hardText = Get-Content -LiteralPath $hard -Raw -Encoding UTF8
    $cardText = Get-Content -LiteralPath $card -Raw -Encoding UTF8
    $start = $hardText.IndexOf('$forbidden = @{')
    $guarded = @()
    if ($start -ge 0) {
        $end = $hardText.IndexOf("`n}", $start)
        $block = $hardText.Substring($start, $end - $start)
        foreach ($found in [regex]::Matches($block, '"(?<name>[a-z0-9-]+)"\s*=')) {
            $guarded += $found.Groups["name"].Value
        }
    }
    if ($guarded.Count -eq 0) {
        Write-Host "no forbidden table, so nothing is guarded against a rule widening"
        exit 1
    }
    $unclaimed = @($guarded | Where-Object { -not $cardText.Contains($_) })
    if ($unclaimed.Count -gt 0) {
        $unclaimed | ForEach-Object { Write-Host ("  the boundary table does not name the guard on " + $_) }
        Write-Host "a document claims a guard it does not list"
        exit 1
    }
    Write-Host ("every forbidden guard is named in the boundary table: " + ($guarded -join ", "))
}

# The model card says how many screens the weights were trained on, and that number lives in the model file too.
# A card that quotes a corpus the weights never saw is the same defect as a front page naming an old release.
$weights = Join-Path $repo "models\page.json"
$cardPath = Join-Path $repo "docs\model-card.md"
if ((Test-Path $weights) -and (Test-Path $cardPath)) {
    $trained = (Get-Content -LiteralPath $weights -Raw | ConvertFrom-Json).trainedOn
    $cardText = Get-Content -LiteralPath $cardPath -Raw -Encoding UTF8
    $quoted = @([regex]::Matches($cardText, '(\d+) screens from') | ForEach-Object { [int]$_.Groups[1].Value })
    if ($quoted.Count -eq 0) {
        Write-Host "the model card no longer says how many screens the weights were trained on"
        exit 1
    }
    $wrong = @($quoted | Where-Object { $_ -ne $trained })
    if ($wrong.Count -gt 0) {
        Write-Host ("  the card quotes {0} screens; the weights were trained on {1}" -f ($wrong -join ", "), $trained)
        Write-Host "the model card quotes a training set that is not in the model file"
        exit 1
    }
    Write-Host ("the card's training count matches the weights: {0} screens" -f $trained)
}

# The model card states the shape of the weights: how many features, and how many parameters. Both are computed
# from the file, so a trainer that adds a feature and leaves the card describing the previous shape fails here.
# This is the pair that drifts silently: no score changes when the card is wrong about the model it is scoring.
if ((Test-Path $weights) -and (Test-Path $cardPath)) {
    $model = Get-Content -LiteralPath $weights -Raw | ConvertFrom-Json
    $featureCount = @($model.weights).Count
    $kindCount = @($model.kinds).Count
    $parameterCount = $featureCount * $kindCount + $kindCount
    $shapeText = Get-Content -LiteralPath $cardPath -Raw -Encoding UTF8
    $claimedFeatures = if ($shapeText -match '\| features \| (\d+)') { [int]$Matches[1] } else { -1 }
    $claimedParameters = if ($shapeText -match '\| parameters \| (\d+)') { [int]$Matches[1] } else { -1 }
    if ($claimedFeatures -lt 0 -or $claimedParameters -lt 0) {
        Write-Host "the model card no longer states the shape of the weights"
        exit 1
    }
    if ($claimedFeatures -ne $featureCount -or $claimedParameters -ne $parameterCount) {
        Write-Host ("  the card says {0} features and {1} parameters; the weights are {2} x {3} + {3} = {4}" -f $claimedFeatures, $claimedParameters, $featureCount, $kindCount, $parameterCount)
        Write-Host "the model card describes a different model from the one in the repository"
        exit 1
    }
    Write-Host ("the card's model shape matches the weights: {0} features, {1} parameters" -f $featureCount, $parameterCount)
}

# The ledger's contract is that every check carries the failure it actually printed, and it is trusted rather than
# checked. This checks it: every row has to contain a quoted message.
#
# One row is excepted. `check-audit`'s violation is produced by hand-building a page whose span runs past its line,
# which no command exports, so its row names the mutation instead of quoting the message. That is a weaker claim and
# it is listed as such rather than glossed.
$quotesNotNeeded = @{}
$ledger = Join-Path $repo "docs\checks.md"
if (Test-Path $ledger) {
    $silent = @()
    $seen = 0
    foreach ($line in Get-Content -LiteralPath $ledger) {
        if ($line -notmatch '^\|\s*`(?<name>check-[a-z-]+)`') { continue }
        $seen++
        $name = $Matches["name"]
        if ($quotesNotNeeded.ContainsKey($name)) { continue }
        if ($line -notmatch 'printed "') { $silent += ("  " + $name + " states what was broken but not what it printed") }
    }
    if ($seen -lt 10) {
        Write-Host ("only {0} ledger rows were read, so the ledger is not the table this checked" -f $seen)
        exit 1
    }
    if ($silent.Count -gt 0) {
        $silent | ForEach-Object { Write-Host $_ }
        Write-Host "a check's proof is a claim about a message that was never quoted"
        exit 1
    }
    if ($quotesNotNeeded.Count -eq 0) { Write-Host "every ledger row quotes the failure it printed" } else { Write-Host ("every ledger row quotes the failure it printed, except: " + (($quotesNotNeeded.Keys | Sort-Object) -join ", ")) }
}

Write-Host "every front page opens with a sentence"