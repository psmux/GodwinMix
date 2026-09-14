"""The `claude` and `codex` drivers: a real agent, through a real MCP server.

Not run in CI. They cost money, they need a key, and a suite whose result moves
when a model is retrained is a measurement of the model rather than of the
mixer. They are here so the same 34 cases can be pointed at a model on the day
somebody wants the number, and `evals/README.md` says how.

Both work the same way. The core is already running; this writes an MCP server
config pointing `gmx mcp` at it, hands the CLI one instruction, and reads the
cost and token counts out of whatever the CLI prints.
"""

import json
import os
import pathlib
import re
import subprocess

# What each CLI wants. Kept in a table rather than in two functions, because
# the only thing that differs between them is the spelling.
TOOLS = {
    "claude": {
        "binary": "claude",
        "argv": lambda prompt, config, model: [
            "claude", "-p", prompt,
            "--mcp-config", str(config),
            "--allowed-tools", "mcp__godwinmix",
            "--output-format", "json",
        ] + (["--model", model] if model else []),
        "config": lambda command: {
            "mcpServers": {"godwinmix": {"command": command[0], "args": command[1:]}}
        },
    },
    "codex": {
        "binary": "codex",
        "argv": lambda prompt, config, model: [
            "codex", "exec", prompt, "--config", str(config), "--json",
        ] + (["--model", model] if model else []),
        "config": lambda command: {
            "mcp_servers": {"godwinmix": {"command": command[0], "args": command[1:]}}
        },
    },
}

PROMPT = """\
You are operating a live video mixer through the GodwinMix MCP tools.

{instruction}

Do it now, with the tools. If the right answer is to do nothing, do nothing and
say why. Do not ask a follow up question; there is nobody to answer it.
"""


def drive(tool, core, case, options):
    spec = TOOLS[tool]
    if not shutil_which(spec["binary"]):
        raise SystemExit(
            f"the {tool} CLI is not on PATH. See evals/README.md; `--driver scripted` "
            "needs nothing installed."
        )
    from run import find_binary

    command = [
        str(find_binary("gmx")), "mcp",
        "--url", core.base.rsplit("/api/", 1)[0],
        "--token", core.token,
    ]
    config = core.dir / f"{tool}-mcp.json"
    config.write_text(json.dumps(spec["config"](command), indent=2))
    prompt = PROMPT.format(instruction=case["instruction"])
    if case.get("interrupt"):
        prompt += "\nPart way through, this arrives: " + case["interrupt"]["instruction"] + "\n"

    finished = subprocess.run(
        spec["argv"](prompt, config, options.model),
        capture_output=True,
        text=True,
        timeout=300,
        env={**os.environ, "GODWINMIX_TOKEN": core.token},
    )
    return read_report(finished.stdout, finished.stderr)


def read_report(stdout, stderr):
    """Cost and tokens, where the CLI says them, and nothing invented.

    Both tools print a JSON summary; the keys have moved before and will again,
    so anything not found is reported as absent rather than as zero. A nil cost
    and a zero cost are different claims.
    """
    report = {"tool_calls": [], "cost_usd": None, "tokens": None}
    for line in reversed(stdout.splitlines()):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(value, dict):
            continue
        for key in ("total_cost_usd", "cost_usd", "costUSD"):
            if key in value:
                report["cost_usd"] = float(value[key])
        usage = value.get("usage") or value
        total = 0
        for key in ("input_tokens", "output_tokens", "total_tokens"):
            if isinstance(usage.get(key), int):
                total += usage[key]
        if total:
            report["tokens"] = total
        if report["cost_usd"] is not None or report["tokens"]:
            break
    for match in re.finditer(r"mcp__godwinmix__([a-z_]+)", stdout):
        report["tool_calls"].append({"method": match.group(1)})
    return report


def shutil_which(name):
    import shutil

    return shutil.which(name)
