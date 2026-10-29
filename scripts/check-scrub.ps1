# Checks the promise a shared training pair makes: no account name, and the schema still intact.
#
# The second half was missing until a pair was exported and read: scrubbing had also renamed the
# catalogue — `moreweb.terminal/v1` became `<user>.terminal/v1` — because the catalogue's name contains
# the organisation's name. Every page was correct; only the identifier a consumer validates against was
# wrong, which no scoring check could see.
#
#   powershell -File scripts/check-scrub.ps1

param()

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-scrub-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$out = Join-Path $work "pairs.jsonl"

# A screen holding a real home path, so the redaction has something to redact.
$session = Get-ChildItem -LiteralPath (Join-Path $repo "eval\unseen") -Directory | Select-Object -First 1
if (-not $session) {
    Write-Host "no recording with a path was found, so nothing was exercised"
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}
& $capture dataset --out $out --scrub --session $session.FullName 2>&1 | Out-Null

$failures = @()
if (-not (Test-Path $out)) {
    Write-Host "the export produced no pairs"
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}
$text = Get-Content -LiteralPath $out -Raw -Encoding UTF8
$pairs = @(($text -split "`n") | Where-Object { $_.Trim() }).Count

Write-Host ("  pairs: {0}" -f $pairs)
if ($pairs -lt 1) { $failures += "no pairs were written, so the promise was not tested" }
if ($text -notmatch "moreweb\.terminal/v1") { $failures += "the catalogue's name was scrubbed away" }
if ($text -match "Users\\+moreweb") { $failures += "an account path survived the scrub" }
if ($text -notmatch "<home>|<user>") { $failures += "nothing was redacted, so the scrubber did not run" }

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a shared pair is not safe to share"
    exit 1
}
Write-Host "a scrubbed pair keeps its catalogue and loses its account"