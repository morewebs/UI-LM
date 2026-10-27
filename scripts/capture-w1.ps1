<#
.SYNOPSIS
    Captures the W1 grok sessions: one recorded session per scripted scenario.

.DESCRIPTION
    Drives Grok Build (the real application) through `ui-lm-capture record` at
    50x160, one keystroke script per scenario, and writes each session to
    `sessions/w1/<name>/`. Afterwards it rewrites `docs/reports/w1-index.json`:
    one row per session with its frame count, unique-screen count, the frames drawn
    on the alternate screen, and the wall-clock timeline of frame arrivals.

    Everything here was shaped by one probe session against the real application:

    * A session only ends when the application leaves. The TUI ignores Ctrl-C,
      Ctrl-D and a typed `/quit` at an idle prompt, and a capture that asks it to
      leave with any of those never ends - and a killed capture writes no
      `session.json`. `/exit` is the application's own way out; the recorder then
      ends by itself ("the program exited") and finishes the session. So every
      scenario's key list ends with an exit tail: two Escapes to close whatever is
      open, then `/exit` twice - the first is swallowed if a turn was generating.

    * `NO_COLOR` is cleared before any session. This harness exports `NO_COLOR=1`,
      and grok honours it by drawing without colour (`grok doctor` says so). A
      capture of an application drawing in one default colour is not a capture of
      what a user sees, and the style half of every frame would be lost. The rig
      already promises a capable terminal through `TERM`/`COLORTERM`; this clears
      the one variable that contradicts it.

    * A frame only exists at a settled screen, so every scenario waits for one.
      The scenarios that produce many frames are the ones that reach a pause the
      application then holds: the welcome card, an open overlay, a finished turn.

    Every session runs with a scratch directory as its working directory, never
    this repository, so no capture can modify our own files. The scratch directory
    is a small git repository, because the diff, plan and approval screens only
    exist for a project the application has a working tree for.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/capture-w1.ps1
    powershell -ExecutionPolicy Bypass -File scripts/capture-w1.ps1 -Only s20-prompt-text -Tag b
#>
[CmdletBinding()]
param(
    # Comma-separated scenario names to run. Empty runs every scenario.
    [string]$Only = '',
    # Appended to each session name, so a re-run lands in a fresh directory.
    [string]$Tag = '',
    # Print what would be run and stop.
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

$RepoRoot     = Split-Path -Parent $PSScriptRoot
$CaptureExe   = Join-Path $RepoRoot 'target\debug\ui-lm-capture.exe'
$GrokExe      = 'C:\Users\moreweb\.grok\bin\grok.exe'
$SessionsRoot = Join-Path $RepoRoot 'sessions\w1'
$IndexPath    = Join-Path $RepoRoot 'docs\reports\w1-index.json'
$WorkRoot     = Join-Path $env:TEMP 'ui-lm-w1'
$Rows         = 50
$Cols         = 160

# The keystrokes that ask the application to leave, appended to every scenario.
# Every scenario starts by answering the application's folder-trust card. A fresh
# scratch directory makes grok ask "Do you trust the contents of this directory?"
# before it will draw its prompt, and the card consumes any keystroke aimed at
# the prompt: without this line every scenario's first instruction is swallowed
# by the card and the session records the card and then nothing. Enter accepts.
$Prologue = @('4500 \r')

$ExitTail = @('1500 \x1b', '1200 \x1b', '1200 /exit\r', '3000 /exit\r')

# The scratch project every session runs in. Small enough to copy per session.
function Get-ScratchFiles {
    @{
        'notes.txt'   = "alpha`r`nbeta`r`ngamma`r`nalpha is the first word`r`n"
        'README.md'   = "# Scratch project`r`n`r`nA directory the capture rig drives grok in.`r`n"
        'config.ini'  = "[app]`r`nname = scratch`r`nverbose = false`r`n"
        'src\app.js'  = "// app`r`nfunction alpha() {`r`n  return 'alpha';`r`n}`r`n`r`nmodule.exports = { alpha };`r`n"
        'src\util.js' = "// util`r`nfunction gamma(n) {`r`n  return n * 2;`r`n}`r`n`r`nmodule.exports = { gamma };`r`n"
    }
}

# One scenario: what to run, what to type, and how long it may take.
#
# `max` is the wall-clock ceiling for the whole session, a guard rather than a
# target: a scenario that reaches its own exit tail ends far sooner. A scenario
# that overruns it is killed, which leaves frames without a record - the index
# reports that as killed rather than dressing it up as a finished session.
#
# `prompt` marks the scenarios that spend quota, because they ask the model to do
# something. The rest only press keys at the UI.
$Scenarios = @(
    @{ name = 's01-startup-idle';           args = @();                               keys = @('8000 /exit\r');                                    max = 45; prompt = $false; note = 'cold start: welcome card, tip line, idle prompt box, status bar' }
    @{ name = 's02-startup-held-idle';      args = @();                               keys = @('22000 /exit\r');                                   max = 60; prompt = $false; note = 'startup held idle 22s: welcome card animation, changelog and tip rotation' }
    @{ name = 's03-startup-fullscreen';     args = @('--fullscreen');                 keys = @('9000 /exit\r');                                    max = 45; prompt = $false; note = 'alternate-screen fullscreen TUI at startup' }
    @{ name = 's04-startup-minimal';        args = @('--minimal');                    keys = @('9000 /exit\r');                                    max = 45; prompt = $false; note = 'scrollback-native (--minimal) rendering at startup' }
    @{ name = 's05-startup-no-alt';         args = @('--no-alt-screen');              keys = @('9000 /exit\r');                                    max = 45; prompt = $false; note = 'inline rendering (--no-alt-screen, the default) at startup' }
    @{ name = 's06-startup-low-effort';     args = @('--reasoning-effort','low','--no-plan'); keys = @('9000 /exit\r');                              max = 45; prompt = $false; note = 'startup with low reasoning effort and no plan mode: status bar and footer differ' }

    @{ name = 's07-slash-echo';             args = @();                               keys = @('8000 /','8000 \x1b','1500 \x03');                  max = 45; prompt = $false; note = 'a bare / typed at the prompt: does it open the documented command menu?' }
    @{ name = 's08-at-echo';                args = @();                               keys = @('8000 @','5000 \x1b','1500 \x03');                  max = 45; prompt = $false; note = 'a bare @ typed at the prompt: does it open the documented file picker?' }
    @{ name = 's09-overlay-ctrl-x';         args = @();                               keys = @('8000 \x18','5000 \x18');                           max = 45; prompt = $false; note = 'Ctrl-X, which the footer advertises as shortcuts, pressed twice' }
    @{ name = 's10-overlay-resume';         args = @();                               keys = @('8000 \x12','6000 \x1b');                           max = 50; prompt = $false; note = 'Ctrl-R resume-session overlay: opened, listing sessions, then closed' }
    @{ name = 's11-command-model';          args = @();                               keys = @('8000 /model\r','7000 \x1b');                       max = 50; prompt = $false; note = '/model submitted: the picker or usage screen it produces' }
    @{ name = 's12-command-feedback';       args = @();                               keys = @('8000 /feedback\r','7000 \x1b');                    max = 50; prompt = $false; note = '/feedback submitted: the feedback screen, before anything is sent' }
    @{ name = 's13-panel-ctrl-t';           args = @();                               keys = @('8000 \x14','5000 \x14');                           max = 45; prompt = $false; note = 'Ctrl-T TODO panel, opened and closed' }
    @{ name = 's14-panel-debug';            args = @();                               keys = @('8000 \x10','5000 \x10');                           max = 45; prompt = $false; note = 'Ctrl-P debug panel, opened and closed' }
    @{ name = 's15-mode-shift-tab';         args = @();                               keys = @('8000 \x1b[Z','4000 \x1b[Z','4000 \x1b[Z');         max = 50; prompt = $false; note = 'Shift-Tab permission-mode cycling: the footer after each press' }
    @{ name = 's16-prompt-multiline';       args = @();                               keys = @('8000 alpha line','1200 \x1b\r','1200 beta line','3000 \x03'); max = 50; prompt = $false; note = 'multiline input: Alt-Enter inserts a newline, nothing is sent' }
    @{ name = 's17-prompt-ctrl-c';          args = @();                               keys = @('8000 abc','3000 \x03','2000 /exit\r');             max = 50; prompt = $false; note = 'does Ctrl-C clear what is in the prompt box, and what the box looks like after' }
    @{ name = 's18-command-plugins';        args = @();                               keys = @('8000 /plugins\r','7000 \x1b');                     max = 50; prompt = $false; note = '/plugins submitted: the plugin list it produces' }
    @{ name = 's19-command-memory';         args = @();                               keys = @('8000 /memory\r','7000 \x1b');                      max = 50; prompt = $false; note = '/memory submitted: the memory browser' }

    @{ name = 's20-prompt-text';            args = @('--reasoning-effort','low','--no-plan'); keys = @('9000 Reply with exactly one word: hello\r','30000 \x1b'); max = 80; prompt = $true; note = 'a short prompt: thinking state, streamed answer, finished turn' }
    @{ name = 's21-prompt-text-fullscreen'; args = @('--fullscreen','--reasoning-effort','low','--no-plan'); keys = @('9000 Reply with exactly one word: hello\r','20000 \x1b'); max = 80; prompt = $true; note = 'the same short prompt on the alternate screen' }
    @{ name = 's22-prompt-text-minimal';    args = @('--minimal','--reasoning-effort','low','--no-plan'); keys = @('9000 Reply with exactly one word: hello\r','20000 \x1b'); max = 80; prompt = $true; note = 'the same short prompt in scrollback-native mode' }
    @{ name = 's23-prompt-two-turns';       args = @('--reasoning-effort','low','--no-plan'); keys = @('9000 Reply with exactly: one\r','15000 Reply with exactly: two\r','20000 \x1b'); max = 105; prompt = $true; note = 'two finished turns in one session: the transcript grows above a reused prompt' }
    @{ name = 's24-tool-read';              args = @('--reasoning-effort','low');      keys = @('9000 Read notes.txt and reply with its last line.\r','45000 \x1b'); max = 90; prompt = $true; note = 'a Read tool call: in progress, then finished with its result' }
    @{ name = 's25-tool-shell';             args = @('--reasoning-effort','low');      keys = @('9000 Use the shell to run echo capture-ok and reply with the output.\r','30000 \x1b'); max = 90; prompt = $true; note = 'a shell tool call and its output' }
    @{ name = 's26-tool-edit-diff';         args = @('--reasoning-effort','low');      keys = @('9000 In notes.txt, replace the word alpha with omega.\r','40000 \x1b'); max = 100; prompt = $true; note = 'an edit, with the inline diff it shows for it' }
    @{ name = 's27-tool-write';             args = @('--reasoning-effort','low');      keys = @('9000 Create made.txt containing the single line: made by capture\r','60000 \x1b'); max = 140; prompt = $true; note = 'a file-creation tool call and its result' }
    @{ name = 's28-tool-search';            args = @('--reasoning-effort','low');      keys = @('9000 Which file mentions gamma? Reply with the file name only.\r','30000 \x1b'); max = 90; prompt = $true; note = 'a search/grep tool call' }
    @{ name = 's29-cancel-turn';            args = @('--reasoning-effort','low');      keys = @('9000 Explain what this directory contains, in ten paragraphs.\r','9000 \x1b'); max = 60; prompt = $true; note = 'a turn cancelled mid-generation: the cancelled state' }
    @{ name = 's30-plan-mode';              args = @('--permission-mode','plan','--reasoning-effort','low'); keys = @('9000 Plan how you would add a LICENSE file to this project.\r','36000 n','6000 \x1b'); max = 105; prompt = $true; note = 'plan mode: the plan it proposes and the prompt to approve it' }
    @{ name = 's32-permission-edit';        args = @('--permission-mode','default','--reasoning-effort','low'); keys = @('9000 Edit notes.txt: replace alpha with omega.\r','35000 \x1b'); max = 100; prompt = $true; note = 'a permission card for an edit, left unanswered' }
    @{ name = 's33-permission-shell';       args = @('--permission-mode','default','--reasoning-effort','low'); keys = @('9000 Run this shell command: echo needs-permission\r','35000 \x1b'); max = 100; prompt = $true; note = 'a permission card for a shell command, left unanswered' }
    @{ name = 's34-question-card';          args = @('--reasoning-effort','low');      keys = @('9000 Ask me which of two names to use for the new file, then stop and wait for my answer.\r','30000 \x1b'); max = 90; prompt = $true; note = 'the application asking the user a question and waiting' }
    @{ name = 's35-permission-approved';    args = @('--permission-mode','default','--reasoning-effort','low'); keys = @('9000 Edit notes.txt: replace alpha with omega.\r','30000 \r','15000 \x1b'); max = 110; prompt = $true; note = 'the same edit permission approved with Enter: the screen after the card is answered' }
    @{ name = 's36-command-usage';         args = @();                               keys = @('8000 /usage\r','9000 \x1b');                        max = 55; prompt = $false; note = '/usage submitted: the token and cost screen the application keeps for a session' }
)

# Scenarios that were attempted and could not be captured. They are listed here,
# not in the table, so a full run does not re-attempt them: each attempt costs
# minutes of wall clock and ends the same way.
$NotCaptured = @(
    [ordered]@{
        scenario = 's31-plan-approve'
        target = 'plan mode: the plan card approved with Enter, and the work that follows'
        attempts = 2
        outcome = 'killed at the ceiling, twice (110 s, then 160 s); frames written, no session.json'
        reason = 'With the card approved the application goes on to a turn that neither finished inside the ceiling nor returned to a prompt the exit tail could leave: Escape and /exit both wedged, as in s30 before it was answered with n. Only declining the plan (s30) reaches a quittable prompt.'
    }
)

# ---------------------------------------------------------------- helpers ----

function Write-Utf8File {
    param([string]$Path, [string]$Text)
    $utf8 = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $utf8)
}

# Runs a native command with its output and error streams discarded, and with the
# error preference temporarily relaxed.
#
# The script runs with $ErrorActionPreference = 'Stop' so that a real failure stops
# the run. A native program's stderr is an error record under that setting, and the
# programs this script drives speak on stderr for ordinary reasons: `git` warns,
# and `taskkill` reports a process it could not reach. Neither is a reason to lose
# an eighteen-minute capture, so both are called through here.
function Invoke-Quietly {
    param([scriptblock]$Command)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Command *> $null } catch {} finally { $ErrorActionPreference = $previous }
}

# Stops a capture that overran its ceiling: the recorder's whole tree, then the
# recorder itself if the tree kill could not reach it.
function Stop-CaptureTree {
    param([int]$Id)
    Invoke-Quietly { & taskkill /PID $Id /T /F }
    Invoke-Quietly { Stop-Process -Id $Id -Force -ErrorAction SilentlyContinue }
}

# Stops a recorder this script started and did not stop itself.
#
# Called from the loop's `finally`, so the cases it covers are the ones the session
# body cannot: a throw between `Start-Process` and the session's own cleanup (the
# frame parsing is the likely one - a session killed at its ceiling can leave a
# half-written last line in `frames.jsonl`), and an operator's Ctrl-C. Either way a
# recorder left alive keeps driving the application with nobody sampling it.
#
# Two searches, because the two failure modes leave different evidence. The handle
# this script holds covers the session that was running. A recorder this script
# started but never recorded a handle for - the throw landed between `Start-Process`
# and the line that assigns it - is found by its executable path instead. Only
# `$CaptureExe` is stopped, so nothing else on the machine is touched; the cost is
# that two runs of this script at once would stop each other's recorder, which is
# already a mistake worth failing loudly.
function Stop-LiveRecorders {
    param([System.Diagnostics.Process]$Own)
    $notes = @()
    if ($null -ne $Own) {
        $alive = $false
        try { $alive = -not $Own.HasExited } catch { $alive = $false }
        if ($alive) {
            Stop-CaptureTree -Id $Own.Id
            $notes += "stopped the recorder this session started (pid $($Own.Id))"
        }
    }
    $strays = @()
    try {
        $recorderName = Split-Path -Leaf $CaptureExe
        $strays = @(Get-CimInstance -ClassName Win32_Process -Filter "Name = '$recorderName'" -ErrorAction Stop |
            Where-Object { [int]$_.ProcessId -ne $PID -and $_.ExecutablePath -eq $CaptureExe })
    } catch {
        $strays = @()
    }
    foreach ($stray in $strays) {
        Stop-CaptureTree -Id ([int]$stray.ProcessId)
        $notes += "stopped a recorder on $CaptureExe that was still running (pid $($stray.ProcessId))"
    }
    if ($notes.Count -eq 0) { return 'no recorder was left running' }
    return ($notes -join '; ')
}

# `1 entry` rather than `1 entries`, for text that goes into the index and the report.
function Format-Count {
    param([int]$Count, [string]$Singular, [string]$Plural)
    if ($Count -eq 1) { return "1 $Singular" }
    return "$Count $Plural"
}

# A Windows command line is one string, and `Start-Process -ArgumentList` joins an
# array with single spaces and quotes nothing. An element that holds a space - a user
# profile under `C:\Users\John Smith`, a scratch directory under a path with one -
# therefore reaches the recorder as two arguments, and the run dies on the first
# session with an argument the recorder does not accept. Each element is quoted here
# the way `CommandLineToArgvW` reads it back: wrap when it is empty or holds a space
# or a quote, double the backslash run that precedes an embedded quote, and double a
# trailing backslash run so it cannot escape the closing quote.
function ConvertTo-CommandLineArgument {
    param([string]$Value)
    if ($Value -ne '' -and $Value -notmatch '[\s"]') { return $Value }
    $sb = New-Object System.Text.StringBuilder
    [void]$sb.Append('"')
    $backslashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq [char]'\') { $backslashes = $backslashes + 1; continue }
        if ($character -eq [char]'"') {
            [void]$sb.Append('\' * ((2 * $backslashes) + 1))
            [void]$sb.Append('"')
            $backslashes = 0
            continue
        }
        if ($backslashes -gt 0) { [void]$sb.Append('\' * $backslashes); $backslashes = 0 }
        [void]$sb.Append($character)
    }
    if ($backslashes -gt 0) { [void]$sb.Append('\' * (2 * $backslashes)) }
    [void]$sb.Append('"')
    return $sb.ToString()
}

# The one argument string `Start-Process` needs, every element quoted.
function ConvertTo-CommandLine {
    param([string[]]$Arguments)
    return (@($Arguments | ForEach-Object { ConvertTo-CommandLineArgument -Value ([string]$_) }) -join ' ')
}

function New-ScratchProject {
    param([string]$Path)
    if (Test-Path -LiteralPath $Path) {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $Path | Out-Null
    $files = Get-ScratchFiles
    foreach ($name in $files.Keys) {
        $full = Join-Path $Path $name
        $parent = Split-Path -Parent $full
        if ($parent -and -not (Test-Path -LiteralPath $parent)) {
            New-Item -ItemType Directory -Force -Path $parent | Out-Null
        }
        Write-Utf8File -Path $full -Text $files[$name]
    }
    # A working tree, because the diff, plan and approval screens only exist for a
    # project the application has something to propose a change to.
    #
    # Every native call goes through Invoke-Quietly. `2>&1 | Out-Null` is not enough:
    # this script runs with $ErrorActionPreference = 'Stop', and PowerShell turns a
    # native program's stderr into an error record, so one program warning aborts the
    # whole run. That is exactly how one run of this script died, in the middle of
    # its thirty-first session.
    Invoke-Quietly { & git -C $Path init -q }
    Invoke-Quietly { & git -C $Path add -A }
    Invoke-Quietly { & git -C $Path -c user.email=capture@example.invalid -c user.name=capture commit -q -m 'scratch baseline' }
}

# Reads a session file without holding the recorder out: it keeps its frame stream
# open for the whole session, and PowerShell's own readers ask for a share mode
# that refuses a file already open for writing.
function Read-SessionText {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    $stream = [System.IO.FileStream]::new($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    $reader = New-Object System.IO.StreamReader($stream, (New-Object System.Text.UTF8Encoding($false)))
    $text = $reader.ReadToEnd()
    $reader.Close()
    $stream.Close()
    return $text
}

# The application persists UI-mode changes into its own config file: during the W1
# probe, `--reasoning-effort low` and a Shift-Tab both rewrote the machine owner's
# defaults in `~/.grok/config.toml`. A capture rig must not change the environment
# the next capture is measured in - a scenario that leaves the application in a
# different mode silently changes what every later session records - so the file is
# snapshotted before the run and put back after every session, and what each session
# wrote is recorded in the index.
function Restore-GrokConfig {
    param([string]$Path, [byte[]]$Backup)
    if ($null -eq $Backup) { return 'no config file to restore' }
    if (Test-Path -LiteralPath $Path) {
        $current = [System.IO.File]::ReadAllBytes($Path)
        if ([System.Convert]::ToBase64String($current) -eq [System.Convert]::ToBase64String($Backup)) {
            return 'unchanged'
        }
        $text = [System.Text.Encoding]::UTF8.GetString($current)
    } else {
        $text = ''
    }
    $mode = '?'
    $effort = '?'
    if ($text -match 'permission_mode\s*=\s*"([^"]*)"') { $mode = $Matches[1] }
    if ($text -match 'default_reasoning_effort\s*=\s*"([^"]*)"') { $effort = $Matches[1] }
    [System.IO.File]::WriteAllBytes($Path, $Backup)
    return "rewritten during the session (permission_mode=$mode default_reasoning_effort=$effort), restored"
}

# The directories a trust list names, and how many of them there are. Counted rather
# than eyeballed, because "the file was restored" is a claim about a count.
function Get-TrustEntries {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return @() }
    $text = [System.IO.File]::ReadAllText($Path)
    return @([regex]::Matches($text, "(?m)^\[folders\.'(.*)'\]") | ForEach-Object { $_.Groups[1].Value })
}

function Get-TrustEntryCount {
    param([string]$Path)
    return @(Get-TrustEntries -Path $Path).Count
}

# Puts one of the application's own configuration files back, byte for byte, and says
# which of the three cases happened: nothing to do, the file was rewritten, or the
# file was deleted during the run and had to be recreated.
function Restore-GrokFile {
    param([string]$Path, [byte[]]$Backup)
    if (-not (Test-Path -LiteralPath $Path)) {
        [System.IO.File]::WriteAllBytes($Path, $Backup)
        return 'deleted during the run, restored'
    }
    $current = [System.IO.File]::ReadAllBytes($Path)
    if ([System.Convert]::ToBase64String($current) -eq [System.Convert]::ToBase64String($Backup)) {
        return 'unchanged'
    }
    [System.IO.File]::WriteAllBytes($Path, $Backup)
    return 'rewritten during the run, restored'
}

# The trust list the application writes when its folder-trust card is answered.
#
# This is the second file the application modifies during a capture, and the one the
# first version of this script missed. Every session starts in a fresh scratch
# directory, so the card is shown for a directory the application has never been
# told to trust, and answering it - which every scenario's prologue Enter does, and
# which earlier passes answered with whatever Enter happened to arrive - writes
# `[folders.'<scratch>']` into `~/.grok/trusted_folders.toml`. Left alone, one run
# adds an entry per session and every re-run adds more.
#
# Restored exactly like the config: after every session, and again in the `finally`.
function Restore-GrokTrust {
    param([string]$Path, [byte[]]$Backup)
    if ($null -ne $Backup) { return (Restore-GrokFile -Path $Path -Backup $Backup) }
    # There was no trust list before the run. One that names only our own scratch
    # directories was created by this capture and is removed; one that names anything
    # else was not, so it is left where it is and named for a human to look at.
    if (-not (Test-Path -LiteralPath $Path)) { return 'no trust list before or after the run' }
    $entries = @(Get-TrustEntries -Path $Path)
    $foreign = @($entries | Where-Object { -not $_.StartsWith($WorkRoot, [System.StringComparison]::OrdinalIgnoreCase) })
    if ($foreign.Count -gt 0) {
        return "no trust list before the run, but one exists now and names $(Format-Count -Count $foreign.Count -Singular 'entry' -Plural 'entries') outside $WorkRoot; left in place for inspection"
    }
    Remove-Item -LiteralPath $Path -Force
    return "no trust list before the run; removed the $(Format-Count -Count $entries.Count -Singular 'entry' -Plural 'entries') the application created for $WorkRoot"
}

# Copies a pre-run snapshot to disk as well as keeping it in memory.
#
# The in-memory copy dies with this process. A run that is force-killed - the console
# window closed, a `Stop-Process` from task manager, a reboot - gets no `finally`, and
# would otherwise leave both configuration files rewritten with nothing to put back.
# A snapshot already on disk is never overwritten: the earliest one is the one taken
# before any capture touched the file. If that early snapshot already names scratch
# directories it is not pristine, and it is called out rather than quietly trusted.
function Save-GrokSnapshot {
    param([string]$Path, [byte[]]$Backup, [string]$Name)
    if ($null -eq $Backup) { return "$Name did not exist before the run, so there was nothing to snapshot" }
    $target = Join-Path $WorkRoot ($Name + '.original.toml')
    if (Test-Path -LiteralPath $target) {
        $scratch = @(Get-TrustEntries -Path $target | Where-Object { $_.StartsWith($WorkRoot, [System.StringComparison]::OrdinalIgnoreCase) })
        if ($scratch.Count -gt 0) {
            return "a snapshot already sits at $target but names $(Format-Count -Count $scratch.Count -Singular 'directory' -Plural 'directories') under ${WorkRoot}: it is not a pristine snapshot, is not used to restore anything, and should be checked by hand"
        }
        return "a snapshot already sits at $target; left as it is"
    }
    [System.IO.File]::WriteAllBytes($target, $Backup)
    return "snapshot of $Path written to $target"
}

# What a session's frames say about themselves, without decoding a grid: the frame
# count, how many distinct screens they show, how many were drawn on the alternate
# screen, and how many rows changed in total.
function Get-FrameStats {
    param([string]$Path)
    $lines = @((Read-SessionText -Path $Path) -split "`n" | Where-Object { $_.Trim().Length -gt 0 })
    $stats = [ordered]@{
        frames = $lines.Count
        unique_screens = 0
        alt_screen_frames = 0
        changed_rows_total = 0
        first_seq = $null
        last_seq = $null
        runs_in_last_frame = 0
    }
    if ($lines.Count -eq 0) { return $stats }
    $seen = New-Object 'System.Collections.Generic.HashSet[string]'
    $sha = [System.Security.Cryptography.SHA256]::Create()
    foreach ($line in $lines) {
        $frame = $line | ConvertFrom-Json
        # What makes two frames the same screen is the cells and the surface they
        # were drawn on. The frame's own numbering, where the cursor sat, and which
        # rows it reported as changed are facts about the frame rather than about the
        # screen, so they are removed before the screen is hashed - otherwise every
        # frame is trivially distinct (its seq alone differs) and the count says
        # nothing.
        $screen = [regex]::Replace($line, '"seq":\d+,', '')
        $screen = [regex]::Replace($screen, '"cursor":(\[[0-9,]+\]|null),', '')
        $screen = [regex]::Replace($screen, '"changed_rows":\[[0-9,]*\],', '')
        $hash = [System.BitConverter]::ToString($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($screen)))
        [void]$seen.Add($hash)
        if ($frame.alt_screen) { $stats.alt_screen_frames = $stats.alt_screen_frames + 1 }
        $stats.changed_rows_total = $stats.changed_rows_total + @($frame.changed_rows).Count
        if ($null -eq $stats.first_seq) { $stats.first_seq = $frame.seq }
        $stats.last_seq = $frame.seq
        $stats.runs_in_last_frame = @($frame.runs).Count
    }
    $sha.Dispose()
    $stats.unique_screens = $seen.Count
    return $stats
}

# --------------------------------------------------------------- the run ----

if (-not (Test-Path -LiteralPath $CaptureExe)) {
    throw "the recorder is not built: $CaptureExe is missing (cargo build --workspace)"
}
if (-not (Test-Path -LiteralPath $GrokExe)) {
    throw "the application is not installed: $GrokExe is missing"
}
New-Item -ItemType Directory -Force -Path $SessionsRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $IndexPath) | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $WorkRoot 'keys') | Out-Null

# NO_COLOR contradicts the terminal the rig promises the child. See the header.
if ($null -ne $env:NO_COLOR) { Remove-Item Env:\NO_COLOR | Out-Null }

# The application's own configuration, as it stands before any session runs.
#
# Two files, not one. `config.toml`, because the application persists its UI mode
# into it: during the W1 probe `--reasoning-effort low` and a Shift-Tab both rewrote
# the machine owner's defaults. And `trusted_folders.toml`, because answering the
# folder-trust card writes a trust entry for the directory that was trusted - and
# every session runs in a fresh scratch directory, so every session answers that
# card. A run that snapshots only the first leaves the second holding one entry per
# session, and the next run adds another one per session on top of those.
$GrokConfig  = Join-Path $env:USERPROFILE '.grok\config.toml'
$GrokTrust   = Join-Path $env:USERPROFILE '.grok\trusted_folders.toml'
$configBackup = $null
if (Test-Path -LiteralPath $GrokConfig) { $configBackup = [System.IO.File]::ReadAllBytes($GrokConfig) }
$trustBackup = $null
if (Test-Path -LiteralPath $GrokTrust) { $trustBackup = [System.IO.File]::ReadAllBytes($GrokTrust) }
$trustCountBefore = Get-TrustEntryCount -Path $GrokTrust

"capture-w1: config: $(Save-GrokSnapshot -Path $GrokConfig -Backup $configBackup -Name 'config')"
"capture-w1: trust:  $(Save-GrokSnapshot -Path $GrokTrust -Backup $trustBackup -Name 'trusted_folders')"
"capture-w1: trust list holds $(Format-Count -Count $trustCountBefore -Singular 'entry' -Plural 'entries') before the run; a full run must leave it holding that many"

$selected = $Scenarios
if ($Only -ne '') {
    $wanted = @($Only -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' })
    $selected = @($Scenarios | Where-Object { $wanted -contains $_.name })
    if ($selected.Count -eq 0) { throw "no scenario matched -Only '$Only'" }
}

# Rows from an earlier run are kept: -Only re-runs a subset, it does not truncate
# the index of the sessions already captured. A scenario that could not be captured
# has no row to keep: a directory without a record was deleted, not indexed.
$indexRows = @()
if (Test-Path -LiteralPath $IndexPath) {
    try { $indexRows = @((Get-Content -LiteralPath $IndexPath -Raw | ConvertFrom-Json).sessions) } catch { $indexRows = @() }
}
$notCapturedNames = @($NotCaptured | ForEach-Object { $_.scenario })
if ($notCapturedNames.Count -gt 0) {
    $indexRows = @(@($indexRows) | Where-Object { $notCapturedNames -notcontains $_.name })
}

$runStarted = Get-Date
"capture-w1: $($selected.Count) scenario(s), recorder $CaptureExe"
"capture-w1: sessions in $SessionsRoot"

# The session loop runs inside `try`, with a `finally` that outlives it. A capture
# changes two files of the machine owner's configuration and leaves a recorder
# driving the application, so the way this loop ends must not decide whether that
# state is put back. Throwing out of the loop (the frame parsing of a session killed
# at its ceiling is the likely place: its last line can be half-written) and an
# operator's Ctrl-C both skip the per-session restore; the `finally` does not.
$activeRecorder = $null

try {
    foreach ($scenario in $selected) {
        $sessionName = $scenario.name
        if ($Tag -ne '') { $sessionName = "$sessionName-$Tag" }
        $outDir = Join-Path $SessionsRoot $sessionName
        $scratch = Join-Path $WorkRoot ('cwd\' + $sessionName)
        $keysPath = Join-Path $WorkRoot ('keys\' + $sessionName + '.txt')
        $stdoutPath = Join-Path $WorkRoot ('keys\' + $sessionName + '.out.txt')
        $stderrPath = Join-Path $WorkRoot ('keys\' + $sessionName + '.err.txt')
        $framesPath = Join-Path $outDir 'frames.jsonl'

        $keyLines = $Prologue + @($scenario.keys) + $ExitTail
        $keyText = ($keyLines -join "`r`n") + "`r`n"

        if ($DryRun) {
            "--- $sessionName (max $($scenario.max)s, prompt=$($scenario.prompt))"
            "    args: $($scenario.args -join ' ')"
            $keyLines | ForEach-Object { "    key: $_" }
            continue
        }

        "== $sessionName -- $($scenario.note)"

        if (Test-Path -LiteralPath $outDir) { Remove-Item -LiteralPath $outDir -Recurse -Force }
        New-ScratchProject -Path $scratch
        Write-Utf8File -Path $keysPath -Text $keyText
        if (Test-Path -LiteralPath $stdoutPath) { Remove-Item -LiteralPath $stdoutPath -Force }
        if (Test-Path -LiteralPath $stderrPath) { Remove-Item -LiteralPath $stderrPath -Force }

        # `--cwd` is not a detail. portable-pty 0.9 gives a child whose command builder
        # names no directory the value of cwd.or(%USERPROFILE%), and the recorder used to
        # pass none, so a session spawned from a scratch directory still ran in the user's
        # home: the tool screens were the home directory's, and one scenario created
        # `made.txt` there. The driver now sets the child's directory itself - the
        # recorder's own cwd unless `--cwd` says otherwise - so it is named here on
        # purpose: `--cwd` to the recorder puts the child in the scratch project whatever
        # directory this script was launched from, and the application's own `--cwd`
        # tells grok the same project explicitly, which is how the captured sessions were
        # driven. Both are passed rather than either alone.
        $captureArgs = @(
            'record', '--rows', "$Rows", '--cols', "$Cols", '--out', $outDir, '--script', $keysPath, '--cwd', $scratch, '--', $GrokExe, '--cwd', $scratch
        ) + @($scenario.args)

        $clock = [System.Diagnostics.Stopwatch]::StartNew()
        # Quoted, not handed over as an array. `-ArgumentList` joins an array with single
        # spaces and quotes nothing, so a scratch directory or a session directory under a
        # path that holds a space (`C:\Users\John Smith\AppData\...`) would arrive at the
        # recorder as two arguments and the run would die here. See
        # ConvertTo-CommandLineArgument.
        $captureCommandLine = ConvertTo-CommandLine -Arguments $captureArgs
        $process = Start-Process -FilePath $CaptureExe -ArgumentList $captureCommandLine -WorkingDirectory $scratch -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
        # Named before the sampling loop, so the `finally` can stop it whatever happens
        # between here and the end of the session.
        $activeRecorder = $process

        # Sample the frame stream while the session runs. The recorder's own record has
        # no timestamps, so the arrival times measured here are the only evidence of
        # what the frame rate actually was.
        $arrivals = New-Object System.Collections.ArrayList
        $lastLength = -1
        $killed = $false
        while (-not $process.HasExited) {
            $length = -1
            if (Test-Path -LiteralPath $framesPath) {
                try { $length = (Get-Item -LiteralPath $framesPath).Length } catch { $length = $lastLength }
            }
            if ($length -ne $lastLength) {
                [void]$arrivals.Add([int]$clock.Elapsed.TotalMilliseconds)
                $lastLength = $length
            }
            if ($clock.Elapsed.TotalSeconds -gt $scenario.max) {
                $killed = $true
                Stop-CaptureTree -Id $process.Id
                break
            }
            Start-Sleep -Milliseconds 100
        }
        $exitCode = $null
        if ($process.HasExited) {
            try { [void]$process.WaitForExit(20000) } catch {}
            $exitCode = $process.ExitCode
        }
        $clock.Stop()
        $wall = [math]::Round($clock.Elapsed.TotalSeconds, 1)

        $stats = Get-FrameStats -Path $framesPath
        $recordPath = Join-Path $outDir 'session.json'
        $hasRecord = Test-Path -LiteralPath $recordPath
        $startedAt = $null
        $endedAt = $null
        if ($hasRecord) {
            try {
                $rec = ((Read-SessionText -Path $recordPath)) | ConvertFrom-Json
                $startedAt = $rec.started_at
                $endedAt = $rec.ended_at
            } catch {}
        }
        $stdoutText = ''
        if (Test-Path -LiteralPath $stdoutPath) { $stdoutText = ((Get-Content -LiteralPath $stdoutPath) -join ' / ') }
        $stderrText = ''
        if (Test-Path -LiteralPath $stderrPath) { $stderrText = ((Get-Content -LiteralPath $stderrPath) -join ' / ') }
        $endsWith = 'unknown'
        if ($killed) { $endsWith = 'killed after ' + $scenario.max + 's' }
        elseif ($stdoutText -match 'the program exited') { $endsWith = 'the program exited' }
        elseif ($stdoutText -match 'interrupted') { $endsWith = 'interrupted' }

        # Put the application's configuration back before anything else runs: what this
        # session did to it must not reach the next one. Both files - the UI mode in
        # `config.toml`, and the trust entry the prologue's Enter has just added for this
        # session's scratch directory.
        $configNote = Restore-GrokConfig -Path $GrokConfig -Backup $configBackup
        $trustNote = Restore-GrokTrust -Path $GrokTrust -Backup $trustBackup

        $row = [ordered]@{
            name = $sessionName
            scenario = $scenario.name
            note = $scenario.note
            grok_args = @($scenario.args)
            prompt_session = [bool]$scenario.prompt
            sessions_dir = 'sessions/w1/' + $sessionName
            rows = $Rows
            cols = $Cols
            frames = $stats.frames
            unique_screens = $stats.unique_screens
            alt_screen_frames = $stats.alt_screen_frames
            changed_rows_total = $stats.changed_rows_total
            first_seq = $stats.first_seq
            last_seq = $stats.last_seq
            runs_in_last_frame = $stats.runs_in_last_frame
            wall_seconds = $wall
            ended = $endsWith
            exit_code = $exitCode
            has_session_json = $hasRecord
            started_at = $startedAt
            ended_at = $endedAt
            frame_arrivals_ms = @($arrivals)
            recorder_stdout = $stdoutText
            recorder_stderr = $stderrText
            config_written = $configNote
            cost = $null
        }
        $indexRows = @(@($indexRows) | Where-Object { $_.name -ne $sessionName }) + @([pscustomobject]$row)

        "   frames=$($stats.frames) unique=$($stats.unique_screens) alt=$($stats.alt_screen_frames) wall=${wall}s ended=$endsWith record=$hasRecord"
        if ($configNote -ne 'unchanged') { "   config: $configNote" }
        if ($trustNote -ne 'unchanged') { "   trust: $trustNote" }
        if ($stderrText -ne '') { "   stderr: $stderrText" }

        # The index is rewritten after every session, so a run that is interrupted
        # still has an index for every session it finished.
        $promptSessions = @(@($indexRows) | Where-Object { $_.prompt_session }).Count
        $index = [ordered]@{
            version = 1
            generated_at = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
            application = $GrokExe
            recorded_with = 'ui-lm-capture record'
            geometry = "${Rows}x${Cols}"
            cost = $null
            cost_note = 'SessionRecord.cost is null for every session: the TUI stream carries no usage events, so nothing priced these runs. Quota must be read from the xAI account; no dollar figure is invented here.'
            exit_code_note = 'exit_code is the recorder''s own exit status, not the application''s. It is null whenever the recorder did not exit - the kill at a scenario''s ceiling is the case this script creates - and it is null in every row of the W1 index. The application''s exit status, where it reports one, survives only as text in recorder_stderr (the two rows carrying "exited with status -1073741510", STATUS_CONTROL_C_EXIT, are s20-prompt-text and s22-prompt-text-minimal).'
            totals = [ordered]@{
                sessions = @($indexRows).Count
                frames = [int](@($indexRows) | Measure-Object -Property frames -Sum).Sum
                unique_screens = [int](@($indexRows) | Measure-Object -Property unique_screens -Sum).Sum
                alt_screen_frames = [int](@($indexRows) | Measure-Object -Property alt_screen_frames -Sum).Sum
                prompt_sessions = $promptSessions
                wall_seconds = [math]::Round((@($indexRows) | Measure-Object -Property wall_seconds -Sum).Sum, 1)
            }
            not_captured = @($NotCaptured)
            sessions = @(@($indexRows) | Sort-Object -Property name)
        }
        Write-Utf8File -Path $IndexPath -Text (($index | ConvertTo-Json -Depth 8) + "`r`n")
    }
}
finally {
    # Whatever ended the loop - the last session, a throw, an operator's Ctrl-C -
    # two things happen before this script exits: both of the application's
    # configuration files go back to the bytes they held before the first session,
    # and no recorder is left alive driving the application with nobody sampling it.
    # The per-session restore in the loop is the normal path; this is the path for
    # everything that skips it.
    $recorderNote = Stop-LiveRecorders -Own $activeRecorder
    if ($null -ne $configBackup) {
        $configFinal = Restore-GrokFile -Path $GrokConfig -Backup $configBackup
    } elseif (Test-Path -LiteralPath $GrokConfig) {
        $configFinal = 'no config.toml before the run, but one exists now; left in place for inspection'
    } else {
        $configFinal = 'no config.toml before or after the run'
    }
    $trustFinal = Restore-GrokTrust -Path $GrokTrust -Backup $trustBackup
    $trustCountAfter = Get-TrustEntryCount -Path $GrokTrust
    "capture-w1: config:   $configFinal -- $GrokConfig"
    "capture-w1: trust:    $trustFinal -- $GrokTrust holds $(Format-Count -Count $trustCountAfter -Singular 'entry' -Plural 'entries'), $(Format-Count -Count $trustCountBefore -Singular 'entry' -Plural 'entries') before the run"
    "capture-w1: recorder: $recorderNote"
}

if ($DryRun) { return }

$runStopped = Get-Date
$totalFrames = 0
foreach ($row in @($indexRows)) { $totalFrames = $totalFrames + [int]$row.frames }
""
"capture-w1: finished. index: $IndexPath"
"capture-w1: $(@($indexRows).Count) session(s) indexed, $totalFrames frame(s)"
"capture-w1: this run took $([math]::Round(($runStopped - $runStarted).TotalSeconds, 1))s of wall time"
"capture-w1: cost is null for every session (no usage events in the TUI stream); read quota from the xAI account"
