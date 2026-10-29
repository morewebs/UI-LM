# Checks the third binary, which ships in every release and had never been run outside the
# measurement that created it.
#
# `ui-lm-validate` compares a recording against a draw-buffer dump from the application's own
# renderer. Its unit tests are thorough, but nothing exercised the command: the dump it reads came
# from a throwaway fork that is not in this repository. This builds a dump by hand, confirms the
# validator agrees, and confirms it *disagrees* when one cell is wrong — because a validator that
# cannot fail is a very expensive way of printing "OK".
#
#   powershell -File scripts/check-validate.ps1
#
# Exit code 1 if the validator reports a mismatch on a matching dump, or misses a wrong cell.

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
$validate = Join-Path $repo "target\debug\ui-lm-validate.exe"
if (-not (Test-Path $capture) -or -not (Test-Path $validate)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-validate-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$session = Join-Path $work "s"
$failures = @()

# A program that prints three characters and waits: no prompt, nothing to predict but "abc".
# The command is written to a script and run from there: passing it inline means PowerShell strips the quotes
# before `wsl` sees them, and `sh -c printf abc; sleep 3` splits into two commands. That made this check compare
# a screen holding more than "abc" against a dump that holds only those three cells — three cells disagreed, on
# every run, and the message read like an emulator fault rather than a quoting one.
$script = Join-Path $work "print.sh"
Set-Content -LiteralPath $script -Value "printf abc`nsleep 3`n" -Encoding ASCII -NoNewline
$wslPath = (& wsl.exe -e wslpath -a ($script.Replace('\', '/'))) 2>$null
if (-not $wslPath) { $wslPath = "/mnt/" + ($script.Substring(0,1).ToLower()) + ($script.Substring(2).Replace('\', '/')) }
& $capture record --rows 10 --cols 40 --out $session --stop-after 8000 -- wsl.exe -e sh $wslPath *> $null
if (-not (Test-Path (Join-Path $session "session.json"))) {
    Write-Host "the recording failed, so the validator was not exercised"
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}

function New-Dump($path, $first) {
    $cells = @()
    $letters = @($first, 'b', 'c')
    for ($i = 0; $i -lt 3; $i++) {
        $cells += [pscustomobject]@{
            row = 0; col = $i; symbol = [string]$letters[$i]
            fg = 'reset'; bg = 'reset'
            bold = $false; italic = $false; underline = $false; reverse = $false
        }
    }
    $line = [pscustomobject]@{ seq = 0; rows = 10; cols = 40; cells = $cells } | ConvertTo-Json -Compress -Depth 5
    Set-Content -LiteralPath $path -Value $line -Encoding ASCII
}

$good = Join-Path $work "dump.jsonl"
$bad = Join-Path $work "wrong.jsonl"
New-Dump $good 'a'
New-Dump $bad 'x'

$agreed = (& $validate compare --ours $session --theirs $good 2>&1 | Select-Object -Last 1) -join ""
$agreedCode = $LASTEXITCODE
Write-Host ("matching dump: {0}" -f $agreed.Trim())
if ($agreedCode -ne 0) { $failures += "the validator disagreed with a dump that matches the screen" }

$disagreed = (& $validate compare --ours $session --theirs $bad 2>&1 | Select-Object -Last 1) -join ""
$disagreedCode = $LASTEXITCODE
Write-Host ("one wrong cell: {0}" -f $disagreed.Trim())
if ($disagreedCode -eq 0) { $failures += "the validator accepted a dump with a wrong cell" }

Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "the validator agrees on a match and disagrees on a difference"
