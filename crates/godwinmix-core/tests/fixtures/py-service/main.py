"""A service plugin with no imports outside the standard library.

The control channel is stderr; stdout is not used. `echo` answers with words
that are not ASCII, so a channel read in the wrong encoding shows; `raw`
writes a line that is not UTF-8 at all, so the host's handling of one shows.
`shutdown` lingers before it exits, the way a plugin closing files does.
"""
import json
import os
import sys
import time

WORDS = "the Lord’s “prayer”, café"
LINGER = float(os.environ.get("PYSERVICE_LINGER", "1.5"))


def send(obj):
    sys.stderr.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stderr.flush()


def core_info():
    """Call the core back the way a service does: GMX_RPC's host, GMX_TOKEN."""
    import urllib.error
    import urllib.request
    rpc = os.environ.get("GMX_RPC", "")
    if not rpc:
        return {"error": "GMX_RPC is empty"}
    url = "http://" + rpc.split("://", 1)[1].rsplit("/rpc", 1)[0] + "/api/v1/core/info"
    req = urllib.request.Request(url, headers={"Authorization": "Bearer " + os.environ.get("GMX_TOKEN", "")})
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return {"status": r.status, "url": url}
    except urllib.error.HTTPError as e:
        return {"status": e.code, "url": url, "answer": e.read().decode("utf-8", "replace")[:300]}
    except Exception as e:
        return {"error": str(e), "url": url}


def tool(rid, params):
    name, args = params.get("name"), params.get("arguments") or {}
    if name == "core":
        args = core_info()
    if name == "raw":
        sys.stderr.flush()
        sys.stderr.buffer.write(b'{"note":"Lord\x92s"}\n')
        sys.stderr.buffer.flush()
    env = {k: os.environ.get(k, "") for k in ("GMX_RPC", "PYTHONUTF8", "PYTHONIOENCODING")}
    env["token"] = bool(os.environ.get("GMX_TOKEN"))
    send({"jsonrpc": "2.0", "id": rid, "result": {
        "content": [{"type": "text", "text": WORDS}],
        "structuredContent": {"words": WORDS, "env": env, "arguments": args, "pid": os.getpid()},
        "isError": False}})


def main():
    send({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "plugin": "pyservice", "version": "0.1.0", "api": 1, "transports": [],
        "provides": [{"kind": "service", "id": "service", "capabilities": ["health"]}]}})
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        rid, method = msg.get("id"), msg.get("method")
        if rid == 0 and "result" in msg:
            send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
        elif method == "tool.call":
            tool(rid, msg.get("params") or {})
        elif method == "health":
            send({"jsonrpc": "2.0", "id": rid, "result": {"state": "ok", "detail": "fine"}})
        elif method == "shutdown":
            time.sleep(LINGER)
            if rid is not None:
                send({"jsonrpc": "2.0", "id": rid, "result": {}})
            return
        elif rid is not None:
            send({"jsonrpc": "2.0", "id": rid, "result": {}})


if __name__ == "__main__":
    main()
