#!/usr/bin/env python3
"""Generate the typed part of every client library from `protocol.json`.

    python3 clients/gen/generate.py            # write all three
    python3 clients/gen/generate.py --check    # fail if what is committed is stale
    python3 clients/gen/generate.py --lang ts  # one language

The contract is `protocol.json` at the repository root, which the core writes
out of `core.api`. A method added to the core lands in every library by running
this once and committing what changes. Nothing here needs a toolchain beyond
the Python that is already required to run the smoke tests.
"""

import argparse
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import model as model_mod  # noqa: E402
import py as py_backend  # noqa: E402
import rs as rs_backend  # noqa: E402
import ts as ts_backend  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(HERE))

TARGETS = {
    "ts": ("clients/typescript/src/generated/protocol.ts", ts_backend.render),
    "python": ("clients/python/godwinmix/_generated.py", py_backend.render),
    "rust": ("crates/godwinmix-client/src/generated.rs", rs_backend.render),
}


def build(lang, protocol):
    path, render = TARGETS[lang]
    return os.path.join(ROOT, path), render(protocol)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--lang", choices=sorted(TARGETS) + ["all"], default="all")
    ap.add_argument("--check", action="store_true", help="write nothing, report drift instead")
    ap.add_argument("--protocol", default=os.path.join(ROOT, "protocol.json"))
    args = ap.parse_args()

    protocol = model_mod.load(args.protocol)
    langs = sorted(TARGETS) if args.lang == "all" else [args.lang]

    stale = []
    for lang in langs:
        path, text = build(lang, protocol)
        if args.check:
            current = None
            if os.path.exists(path):
                with open(path, "r", encoding="utf-8") as fh:
                    current = fh.read()
            if current != text:
                stale.append(os.path.relpath(path, ROOT))
            continue
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)
        print(f"wrote {os.path.relpath(path, ROOT)} ({len(text.splitlines())} lines)")

    if stale:
        print("stale, regenerate with `python3 clients/gen/generate.py`:", file=sys.stderr)
        for path in stale:
            print("  " + path, file=sys.stderr)
        return 1
    if args.check:
        print(f"api_level {protocol.api_level}: {', '.join(langs)} match protocol.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
