# Every page the recorder produces must satisfy the envelope's schema (rounds 150-152). The schema was derived from the corpus,
# not from an example, and this check is what keeps the two in step. The work is Python because the validator is.
$env:PYTHONIOENCODING = "utf-8"
& python (Join-Path $PSScriptRoot "check-schema.py")
exit $LASTEXITCODE
