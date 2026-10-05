"""`agent.setup` through a core started by dev/agents_e2e.py, into the core's
own made up home, so nothing lands in the runner's real one."""

import json
import os
import time
import urllib.request


def setup_steps(step, core):
    """`agent.setup` through the core, into the core's own made up home."""
    def call(params):
        req = urllib.request.Request(core.url + "/api/v1/agent/setup", data=json.dumps(params).encode(),
                                     headers={"Authorization": "Bearer " + core.token, "Content-Type": "application/json"})
        # A setup run twice is a setup run once, so a connection a VPN on
        # the runner reset is simply asked again.
        for attempt in range(3):
            try:
                with urllib.request.urlopen(req, timeout=30) as r:
                    return json.loads(r.read())
            except ConnectionResetError:
                if attempt == 2:
                    raise
                time.sleep(1)

    def a_dry_run_writes_nothing():
        plan = call({"tool": "opencode", "dry_run": True})
        assert plan.get("dry_run") and plan["writes"], plan
        assert not os.path.exists(os.path.join(core.home, ".config", "opencode")), "a dry run wrote files"

    def a_setup_writes_the_entry_and_the_skills():
        done = call({"tool": "opencode"})
        config = os.path.join(core.home, ".config", "opencode", "opencode.json")
        with open(config, encoding="utf-8") as f:
            entry = json.load(f)["mcp"]["godwinmix"]
        assert entry["command"][1] == "mcp", entry
        skill = os.path.join(core.home, ".config", "opencode", "skills", "godwinmix-design", "SKILL.md")
        assert os.path.isfile(skill), f"no skill at {skill}"
        assert "opencode" in done["start"], done

    step("core: agent.setup dry run writes nothing", a_dry_run_writes_nothing)
    step("core: agent.setup writes the entry and the skills", a_setup_writes_the_entry_and_the_skills)
