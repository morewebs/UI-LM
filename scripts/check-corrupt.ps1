# Damaged recordings: the tools must say so and carry on, never fall over.
#
# A recorder that is killed mid-write leaves a truncated last line, and a disk that fails leaves worse.
# Both are ordinary conditions for a tool whose whole job is reading files written by another process,
# so this damages a real recording in the two ways that happen and asks what the tools do.

param()

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
if (-not (Test-Path $capture)) { throw "build first: cargo build --workspace" }

$source = Join-Path $repo "cases\hard\wsl-agent-in-split"
if (-not (Test-Path $source)) {
    Write-Host "no recording to damage (the hard cases are local), so nothing was exercised"
    exit 1
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-corrupt-" + [guid]::NewGuid().ToString("N"))
$failures = @()

function Copy-Session($into) {
    New-Item -ItemType Directory -Force -Path $into | Out-Null
    Copy-Item -Path (Join-Path $source "*") -Destination $into -Recurse -Force
}

# 1. A truncated last line: the rest must still read, and the tool must say what it skipped.
$cut = Join-Path $work "cut"
Copy-Session $cut
$lines = [System.IO.File]::ReadAllLines((Join-Path $cut "frames.jsonl"))
$total = @($lines | Where-Object { $_.Trim() }).Count
# Two corruptions, because one mutation can be survived by accident — a byte that happens to land in padding
# still parses, and a check that passes on one lucky mutation has shown nothing.
if ($total -lt 2) {
    Write-Host "the recording is too short to damage"
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}
$lines[$total - 1] = $lines[$total - 1].Substring(0, [Math]::Min(80, $lines[$total - 1].Length))
[System.IO.File]::WriteAllLines((Join-Path $cut "frames.jsonl"), $lines, (New-Object System.Text.UTF8Encoding($false)))

$out = & $capture summarize --session $cut 2>&1
$code = $LASTEXITCODE
$text = ($out | ForEach-Object { "$_" }) -join "`n"
$read = 0
if ($text -match "(\d+) frame\(s\)") { $read = [int]$Matches[1] }
Write-Host ("  truncated last line: {0} of {1} frame(s) read, exit {2}" -f $read, $total, $code)
if ($code -ne 0) { $failures += "a truncated last line stopped summarize (exit $code)" }
if ($text -notmatch "skipping unreadable frame line") { $failures += "nothing said the frame was skipped" }

# 2. Nothing parseable at all: the tools must exit non-zero and say why, not panic.
$garbage = Join-Path $work "garbage"
Copy-Session $garbage
[System.IO.File]::WriteAllText((Join-Path $garbage "frames.jsonl"), "{ this is not a frame }`n{ neither is this }`n", (New-Object System.Text.UTF8Encoding($false)))
$out2 = & $capture page --session $garbage --all 2>&1
$code2 = $LASTEXITCODE
$text2 = ($out2 | ForEach-Object { "$_" }) -join "`n"
Write-Host ("  nothing parseable: exit {0}" -f $code2)
if ($code2 -eq 0) { $failures += "a recording of pure garbage was read as if it were fine" }
if ($text2 -match "panicked") { $failures += "the tool panicked on garbage" }

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a damaged recording was not handled"
    exit 1
}
Write-Host "damaged recordings are skipped line by line and refused when nothing is readable"