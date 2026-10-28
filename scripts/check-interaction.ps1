# Checks that typing into the live view actually reaches the program and comes back.
#
# Nothing else in this project tests the interactive path: every other check reads a recording.
# This one runs the two processes the README describes, types into the viewer over HTTP the way
# the browser's box does, and fails unless the program ran the text and the result appeared on
# screen.
#
#   powershell -File scripts/check-interaction.ps1
#
# Exit code 1 if the round trip does not happen.

$ErrorActionPreference = "Continue"
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo "target\debug\ui-lm-capture.exe"
$serve = Join-Path $repo "target\debug\ui-lm-serve.exe"
if (-not (Test-Path $capture) -or -not (Test-Path $serve)) { throw "build first: cargo build --workspace" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("ui-lm-interact-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$session = Join-Path $work "live"
$control = Join-Path $work "keys.txt"
Set-Content -LiteralPath $control -Value "" -Encoding ASCII
$port = 8810
$marker = "INTERACTION-MARKER-" + (Get-Random -Maximum 9999)

$recorder = Start-Process -FilePath $capture -PassThru -NoNewWindow -ArgumentList @(
    "record", "--rows", "24", "--cols", "96", "--out", $session, "--control", $control,
    "--stop-after", "30000", "--", "powershell", "-NoProfile")
Start-Sleep -Seconds 6
$viewer = Start-Process -FilePath $serve -PassThru -NoNewWindow -ArgumentList @(
    "--session", $session, "--keys", $control, "--port", "$port")
Start-Sleep -Seconds 3

$failure = $null
try {
    $before = (Invoke-WebRequest -Uri "http://127.0.0.1:$port/state" -UseBasicParsing -TimeoutSec 5).Content |
        ConvertFrom-Json
    # Exactly what the browser's box sends: keystrokes as the body, ending in Enter.
    $null = Invoke-WebRequest -Uri "http://127.0.0.1:$port/keys" -Method Post -Body "Write-Output $marker`r" `
        -UseBasicParsing -TimeoutSec 5
    Start-Sleep -Seconds 4
    $after = (Invoke-WebRequest -Uri "http://127.0.0.1:$port/state" -UseBasicParsing -TimeoutSec 5).Content |
        ConvertFrom-Json

    if (-not ($after.page -match $marker)) {
        $failure = "the typed text never appeared on screen"
    } elseif ($after.frames -le $before.frames) {
        $failure = "the screen did not gain a frame after typing ($($before.frames) -> $($after.frames))"
    } else {
        Write-Host "typed $marker into the viewer: frames $($before.frames) -> $($after.frames), and the output came back"
    }
} catch {
    $failure = $_.Exception.Message
} finally {
    if (-not $viewer.HasExited) { $viewer.Kill() }
    Start-Sleep -Milliseconds 500
    if (-not $recorder.HasExited) { $recorder.Kill() }
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

if ($failure) {
    Write-Host "interaction failed: $failure"
    exit 1
}
Write-Host "the interactive path works"
