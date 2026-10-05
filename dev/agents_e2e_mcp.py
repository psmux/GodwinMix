"""`godwinmix mcp` driven the way an MCP client drives it, for dev/agents_e2e.py.

One child process for the whole run, JSON-RPC one line each way, exactly as
Claude Code, opencode and Cursor talk to it over stdio.
"""

import json
import subprocess
import threading


class Mcp:
    def __init__(self, gmx, url, token, profile="standard"):
        self.proc = subprocess.Popen(
            [gmx, "mcp", "--url", url, "--token", token, "--profile", profile],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, encoding="utf-8", bufsize=1,
        )
        self.next_id = 0

    def request(self, method, params=None, timeout=60):
        self.next_id += 1
        msg = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            msg["params"] = params
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()
        return self.read(self.next_id, timeout)

    def notify(self, method):
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method}) + "\n")
        self.proc.stdin.flush()

    def read(self, want, timeout):
        """The reply to `want`, skipping the server's own notifications."""
        result = {}

        def pump():
            for line in self.proc.stdout:
                line = line.strip()
                if not line.startswith("{"):
                    raise AssertionError(f"stdout carried something that is not JSON: {line[:200]}")
                message = json.loads(line)
                if message.get("id") == want:
                    result["message"] = message
                    return

        reader = threading.Thread(target=pump, daemon=True)
        reader.start()
        reader.join(timeout)
        if "message" not in result:
            raise AssertionError(f"no answer to request {want} in {timeout}s")
        return result["message"]

    def initialize(self):
        reply = self.request("initialize", {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "agents-e2e", "version": "1"},
        })
        self.notify("notifications/initialized")
        return reply["result"]

    def tools(self):
        return self.request("tools/list")["result"]["tools"]

    def call(self, name, arguments=None, ok=True):
        """A tool's answer as (text, images). `ok` False expects a refusal."""
        reply = self.request("tools/call", {"name": name, "arguments": arguments or {}})
        if "error" in reply:
            raise AssertionError(f"{name}: protocol error {reply['error']}")
        result = reply["result"]
        text = "\n".join(p.get("text", "") for p in result.get("content", []) if p.get("type") == "text")
        images = [p for p in result.get("content", []) if p.get("type") == "image"]
        refused = bool(result.get("isError"))
        if refused == ok:
            raise AssertionError(f"{name} {'refused' if refused else 'did not refuse'}: {text[:600]}")
        return text, images

    def json(self, name, arguments=None):
        text, _ = self.call(name, arguments)
        return json.loads(text)

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=10)
        except Exception:
            self.proc.kill()
