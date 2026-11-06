"""Counts, for each component kind, how many distinct programs have produced it in the corpus on this machine.

The vocabulary rule in the handoff is "no vocabulary from one screen": a kind is added with three screens of evidence,
or not at all. A screen is not a program, though, and a kind seen in eleven recordings of the same CLI is still a kind
seen through one program's habits. This prints the other number - producers, not screens - because the report keeps
asking "does this kind have a second producer" and answering it from memory.

The producer is read two ways, because one way is not enough:

- the **command field** of the session record, which is the only definition of a recording's program that admits
  checking: the screens themselves do not carry a name;
- the **inner program** when the command is a wrapper and its arguments name one after `-e` (`wsl.exe -e vim ...`).
  That recovers vim, top, df and git from the WSL recordings - but not `sh /tmp/ui-lm/wt.sh`, where the program under
  the shell is a whiptail dialog and the record cannot say so.

A producer count is therefore a lower bound whenever a script is in the way, and the two columns are printed together
rather than the better-looking one alone.

Run from the repository root:

    python scripts/measure-producers.py

It prints every kind with its producers under both definitions, the recordings it appears in, and the frames counted;
a kind with one producer is called out, because that is the fact the report previously kept in prose.
"""

import json
import os
import re
import subprocess
import sys

CAPTURE = os.path.join("target", "debug", "ui-lm-capture.exe")
KIND = re.compile(r"([A-Z][A-Za-z]+)=(\d+)")


def sessions():
    for root in ("eval", "cases"):
        if not os.path.isdir(root):
            continue
        for parent, dirs, files in os.walk(root):
            if os.path.basename(parent) == "" and "session.json" in files:
                continue
            if "session.json" in files:
                yield parent


def record_of(session_dir):
    try:
        with open(os.path.join(session_dir, "session.json"), encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, ValueError):
        return None


def command_of(record):
    return (record or {}).get("command") or "(unknown)"


def inner_of(record):
    """The program under a wrapper, when the record names one; the command otherwise."""
    command = command_of(record)
    args = (record or {}).get("args") or []
    if command in ("wsl", "wsl.exe") and "-e" in args:
        after = args.index("-e") + 1
        if after < len(args):
            return "wsl->" + args[after]
    return command


def frames_of(session_dir):
    """The kinds and their counts over every frame, from the reader the project ships."""
    result = subprocess.run(
        [CAPTURE, "frames", "--session", session_dir],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if result.returncode != 0:
        return {}
    counts = {}
    for line in result.stdout.splitlines():
        for name, count in KIND.findall(line):
            counts[name] = counts.get(name, 0) + int(count)
    return counts


def main():
    if not os.path.exists(CAPTURE):
        print("build first: cargo build --workspace")
        return 1

    found = {}
    recordings = 0
    for session_dir in sorted(sessions()):
        recordings += 1
        record = record_of(session_dir)
        command = command_of(record)
        inner = inner_of(record)
        kinds = frames_of(session_dir)
        for kind, count in kinds.items():
            entry = found.setdefault(kind, {"commands": set(), "inner": set(), "sessions": 0, "frames": 0})
            entry["commands"].add(command)
            entry["inner"].add(inner)
            entry["sessions"] += 1
            entry["frames"] += count

    if not found:
        print("no recordings, so nothing was measured")
        return 1

    print(f"{recordings} recording(s) on this machine, {sum(e['sessions'] for e in found.values())} kind appearances")
    print()
    print(f"{'kind':10} {'commands':9} {'inner':6} {'recordings':10} {'frames':7} producers (inner program where named)")
    for kind in sorted(found, key=lambda name: (-len(found[name]["inner"]), name)):
        entry = found[kind]
        print(f"{kind:10} {len(entry['commands']):9} {len(entry['inner']):6} {entry['sessions']:10} {entry['frames']:7} {', '.join(sorted(entry['inner']))}")

    commands_only = {kind: entry for kind, entry in found.items() if len(entry["commands"]) == 1}
    if commands_only:
        print()
        for kind, entry in sorted(commands_only.items()):
            command = next(iter(entry["commands"]))
            print(f"one command field: {kind} is seen only in {command} ({entry['sessions']} recording(s)), "
                  f"whose inner programs are {', '.join(sorted(entry['inner']))}")
    shells = sorted({inner for entry in found.values() for inner in entry["inner"] if inner.endswith("->sh")})
    if shells:
        print()
        print("the wrapper hides a program: " + ", ".join(shells) +
              " are shell scripts, and the program under the shell is not in the record")
    return 0


if __name__ == "__main__":
    sys.exit(main())
