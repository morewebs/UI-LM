"""Every page the recorder produces must satisfy the envelope's schema.

The catalogue's examples describe the format; the schema decides it, and both were derived from the corpus. Validating every
page of every recording means a change to the format that no example covers still fails.
"""
import glob
import json
import os
import subprocess
import sys

import jsonschema

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CAPTURE = os.path.join(ROOT, "target", "debug", "ui-lm-capture.exe")
if not os.path.exists(CAPTURE):
    CAPTURE = os.path.join(ROOT, "target", "release", "ui-lm-capture.exe")
if not os.path.exists(CAPTURE):
    print("no capture binary, so nothing was measured")
    sys.exit(1)
schema = json.load(open(os.path.join(ROOT, "docs", "catalogue", "moreweb.terminal-v1.schema.json"), encoding="utf-8"))
validator = jsonschema.Draft202012Validator(schema)
sessions = []
for base in ("eval", "cases"):
    for root, _, files in os.walk(os.path.join(ROOT, base)):
        if "session.json" in files:
            sessions.append(root)
checked, failures = 0, []
for session in sorted(sessions):
    out = subprocess.run([CAPTURE, "page", "--session", session, "--all"], capture_output=True, text=True, encoding="utf-8")
    for line in out.stdout.split("\n"):
        if not line.strip().startswith("{"):
            continue
        page = json.loads(line)
        for error in validator.iter_errors(page):
            failures.append((os.path.basename(session), "/".join(str(p) for p in error.absolute_path), error.message))
        checked += 1
# The conformance kit is what a runtime author validates first: its stored pages must satisfy the same schema as the
# corpus, or the project's own examples fail the first test the guide tells a stranger to run.
kit = sorted(glob.glob(os.path.join(ROOT, "docs", "conformance", "*", "page.json")))
if not kit:
    print("the conformance kit holds no page, so nothing was checked there")
    sys.exit(1)
for path in kit:
    with open(path, encoding="utf-8") as handle:
        page = json.load(handle)
    name = "conformance/" + os.path.basename(os.path.dirname(path))
    for error in validator.iter_errors(page):
        failures.append((name, "/".join(str(p) for p in error.absolute_path), error.message))
    checked += 1
if checked == 0:
    print("no page was read, so nothing was measured")
    sys.exit(1)
if failures:
    print("%d of %d page(s) do not satisfy the schema" % (len(failures), checked))
    for name, path, message in failures[:5]:
        print("  %s at %s: %s" % (name, path or "(root)", message))
    sys.exit(1)
print("every page satisfies the schema: %d page(s), %d recording(s), %d conformance page(s)" % (checked, len(sessions), len(kit)))
