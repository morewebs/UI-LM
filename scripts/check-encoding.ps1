# Every check that captures a native command's output must set its own decoding first.
#
# Windows PowerShell decodes a native command's stdout with the console's code page. A suite started from a CP437
# terminal measured the terminal instead of the page: `check-html` reported 58 words missing and `check-conformance` a
# page that drew a different screen (round 169). The fix is `scripts/utf8.ps1`, dot-sourced before the first capture.
#
# The fix has a failure mode nothing else catches: a check added later can forget the line, and the suite would only
# notice if that check happened to compare a non-ASCII word - which is exactly the kind of "green until the material
# changes" a check exists to prevent. This one reads the other checks as text and refuses a capture before the encoding.

. (Join-Path $PSScriptRoot "utf8.ps1")
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$scriptsDir = Join-Path $repo "scripts"

# The runner and this check are not captures: the runner reads children through log files, and this check's own
# pattern contains the word `$capture` as text. Counting either would make the number it prints a little less true.
$checks = @(Get-ChildItem -LiteralPath $scriptsDir -Filter "check-*.ps1" |
    Where-Object { $_.Name -ne "check-everything.ps1" -and $_.Name -ne "check-encoding.ps1" })
if ($checks.Count -lt 20) {
    Write-Host ("only {0} check(s) were found, so the scan did not see the suite" -f $checks.Count)
    exit 1
}

$capture = @()
$failures = @()
foreach ($check in $checks) {
    $lines = @(Get-Content -LiteralPath $check.FullName -Encoding UTF8)
    $firstCapture = -1
    $firstEncoding = -1
    for ($i = 0; $i -lt $lines.Count; $i++) {
        # A capture is a native program whose stdout the script reads: the binary, python, or wsl. The encoding must
        # be set first; after the capture it would only cover the second one.
        if ($firstCapture -lt 0 -and $lines[$i] -match '(\$capture|& python|& wsl)') {
            $firstCapture = $i
        }
        if ($firstEncoding -lt 0 -and $lines[$i] -match 'utf8\.ps1') {
            $firstEncoding = $i
        }
    }
    if ($firstCapture -lt 0) { continue }
    $capture += $check.Name
    if ($firstEncoding -lt 0) {
        $failures += ("{0}: captures a native command and never loads scripts/utf8.ps1" -f $check.Name)
    } elseif ($firstEncoding -gt $firstCapture) {
        $failures += ("{0}: loads scripts/utf8.ps1 after its first capture" -f $check.Name)
    }
}

if ($capture.Count -lt 10) {
    Write-Host ("only {0} check(s) capture a native command, so the ordering was not really checked" -f $capture.Count)
    exit 1
}
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host ("  " + $_) }
    Write-Host "a check would decode a native command with the terminal's code page"
    exit 1
}
Write-Host ("every check sets its decoding before the first capture: {0} check(s), {1} with a capture" -f $checks.Count, $capture.Count)
