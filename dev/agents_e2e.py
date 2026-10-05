#!/usr/bin/env python3
"""What an AI agent does to a mixer, without the model: MCP and `gmx tool`.

Usage:
  dev/agents_e2e.py --url http://127.0.0.1:8080 --token TOKEN [--gmx PATH]
  dev/agents_e2e.py --start [--gmx PATH]     start a throwaway core first

The calls are the ones Claude Code, opencode and pi made when they were asked
for a lower third, a ticker, a cut and "what is on air": read the state, make
a scene of the source on air, add a template graphic and a ticker, change the
words, look at the programme, and reach a tool outside the hot list through
`call_tool`. Then the same through `gmx tool`, the way pi runs them. Every
step prints ok or fails the run. Standard library only, so it runs on any CI
runner with Python 3.8 or newer. The sources and scenes it adds carry the
run's number in their names and are left behind, so point it at a test core.
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from agents_e2e_cli import cli_steps  # noqa: E402
from agents_e2e_core import Core  # noqa: E402
from agents_e2e_mcp import Mcp  # noqa: E402
from agents_e2e_mcp_steps import mcp_steps  # noqa: E402
from agents_e2e_setup import setup_steps  # noqa: E402

RUN = f"e2e{os.getpid()}"
FAILED = []


def step(name, fn):
    print(f"{name:<62}", end="", flush=True)
    try:
        fn()
        print("ok")
    except Exception as e:  # noqa: BLE001 - every failure is reported the same way
        print("FAIL")
        print(f"    {e}", file=sys.stderr)
        FAILED.append(name)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--url", default=os.environ.get("GODWINMIX_URL"))
    ap.add_argument("--token", default=os.environ.get("GODWINMIX_TOKEN", ""))
    ap.add_argument("--gmx", default=os.environ.get("GMX", "gmx"), help="the gmx or godwinmix executable")
    ap.add_argument("--start", action="store_true", help="start a throwaway core with two test cameras")
    args = ap.parse_args()
    if os.sep in args.gmx or "/" in args.gmx:
        # The core runs in a folder of its own, where a relative path means nothing.
        args.gmx = os.path.abspath(args.gmx)
    core = None
    if args.start:
        core = Core(args.gmx).wait()
        args.url, args.token = core.url, core.token
        time.sleep(3)
    if not args.url:
        ap.error("give --url and --token, or --start")
    print(f"agents end to end against {args.url}\n")
    mcp = Mcp(args.gmx, args.url, args.token)
    try:
        mcp_steps(step, mcp, RUN)
    finally:
        mcp.close()
    cli_steps(step, args.gmx, args.url, args.token, RUN)
    if core:
        setup_steps(step, core)
        core.stop()
    print(f"\n{len(FAILED)} failed" if FAILED else "\nall passed")
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
