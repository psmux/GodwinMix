#!/usr/bin/env python3
"""Count the tools `gmx mcp` puts in front of an agent.

Speaks the three lines of MCP that matter: initialize, initialized,
tools/list. Prints the count and nothing else, so the shell can compare it.

Usage: smoke_mcp.py <gmx> <url> <token> <standard|minimal>
"""

import json
import subprocess
import sys


def main():
    gmx, url, token, profile = sys.argv[1:5]
    child = subprocess.Popen(
        [gmx, "mcp", "--url", url, "--token", token, "--profile", profile],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    requests = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "smoke", "version": "0"},
            },
        },
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    ]
    body = "".join(json.dumps(r) + "\n" for r in requests)
    try:
        out, err = child.communicate(body, timeout=30)
    except subprocess.TimeoutExpired:
        child.kill()
        print("gmx mcp did not answer in time", file=sys.stderr)
        return 1

    for line in out.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        message = json.loads(line)
        if message.get("id") == 2:
            tools = message.get("result", {}).get("tools")
            if tools is None:
                print(f"tools/list answered {message}", file=sys.stderr)
                return 1
            print(len(tools))
            return 0
    print(f"no tools/list answer. stderr: {err[-400:]}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
