# Windows PowerShell decodes a native command's stdout with the console's output code page. When the suite was started
# from a CP437 terminal, `check-html` reported 58 words missing and `check-conformance` a page that drew a different
# screen - both were measuring the terminal, not the page (round 169). Setting `[Console]::OutputEncoding` in the check
# that captures the output is the only place it can be set without changing the console the caller is sitting in: a
# child started with `-File` initialises its own decoding, and a child started with `-Command` cannot carry a script's
# exit code (a normal completion would return the last native program's code, which two checks produce on purpose).
#
# Every check dot-sources this before its first native command.
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
