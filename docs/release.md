# Releasing

The order matters, and the reason is a check. `check-docs` compares the version a front page names with the
newest tag in the repository, so **a tag pushed before its page is updated leaves `main` wrong** — correct for a
few minutes, which is exactly the kind of wrong that survives a release nobody was watching.

## The order

1. **The suite is green**, full run, not quick:
   `powershell -File scripts/check-everything.ps1`
2. **Update the front pages first**: the state page and the month review name the new version and count the
   releases. Then `check-docs` passes *before* the tag exists — it compares against the newest tag, so this step
   is a no-op for the version row until the tag lands, and it is the numbers that move now.
3. **Gate and commit** the documents: `scripts/gate.ps1 -Commit "docs: vX.Y.Z"`. Nothing is committed unless
   clippy, formatting, tests, `check-docs` and `check-readme` all pass.
4. **Tag and push**, last: `git tag -a vX.Y.Z -m "…"` then `git push origin main --tags`. The commit that carries
   the tag names the version already, so `main` is never behind the tag.
5. **Wait for the three assets**, then run `scripts/check-release.ps1 -Tag vX.Y.Z`, which downloads the archive,
   runs it, and fails if the download does not behave like a stranger's.
6. **Download it once by hand** and ask it what it is: `ui-lm-capture version` prints the commit and a digest of
   the working tree. That is what the stamp exists for.

## What each step catches

| step | catches |
|---|---|
| 1 | a release of code the suite never saw |
| 2 | a front page naming the previous version |
| 3 | formatting, lints, tests, and any document count that drifted |
| 4 | nothing — it is the point at which mistakes stop being visible |
| 5 | an archive missing a platform, or one that does not run |
| 6 | a binary built from a tree that is not the one being released |

## Why this page exists

Twenty-four releases were made by hand, in an order that put the tag first, and `check-docs` was green each time
because it was run *after* the pages were fixed. The gap was not in the check but in the sequence — and a sequence
that lives in someone's habit is the same problem as a rule that lives in two places.
