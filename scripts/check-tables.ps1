# The documented rule and the shipped reader must agree about every component in the corpus.
#
# `scripts/measure-tables.py --disagreements` applies the ADR 0008 predicate to the rows the binary emits and compares the
# answer with the component the reader chose. A mismatch is one of them being wrong: the predicate sliding a window where
# the reader judges a whole block reported a table inside an agent's listing and cost two rounds to explain (round 14).

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$script = Join-Path $repo "scripts\measure-tables.py"
if (-not (Test-Path $script)) {
    Write-Host "the measurement script is missing, so nothing was compared"
    exit 1
}
if (-not (Test-Path (Join-Path $repo "target\debug\ui-lm-capture.exe"))) {
    Write-Host "no capture binary, so nothing was measured"
    exit 1
}
Push-Location $repo
try {
    $out = & python $script --disagreements 2>&1
    $code = $LASTEXITCODE
} finally {
    Pop-Location
}
$out | ForEach-Object { Write-Host ("  " + $_) }
if ($code -ne 0) {
    Write-Host "the rule and the reader disagree about a component"
    exit 1
}
Write-Host "the rule and the reader agree about every component in the corpus"
exit 0
