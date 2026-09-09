#!/usr/bin/env python3
"""Keep the programme on whatever the quiz is doing.

Every few seconds: ask the quiz which championship match is live. While one
is, the mixer's "match" source is that match's watch page and it is on
programme; when none is, the championship page is. Matches come and go every
few minutes under the simulator, and their watch pages are per match, so
nobody could keep up by hand. Standard library only; runs in python:alpine.
"""
import json, os, sys, threading, time, urllib.request, urllib.error
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

QUIZ = os.environ.get("QUIZ_URL", "http://btq:5001")
MIXER = os.environ.get("MIXER_URL", "http://lbx:8080")
MIXER_TOKEN = os.environ.get("MIXER_TOKEN", "")
CHAMP = os.environ.get("CHAMPIONSHIP_ID", "")
PERIOD = float(os.environ.get("PERIOD_SECS", "4"))
BIND = os.environ.get("DIRECTOR_BIND", "0.0.0.0:8090")
STREAM_URL = os.environ.get("STREAM_URL", "")

# The switch the quiz's admin panel flips. On: follow the live match. Off:
# cut to black and leave the programme alone until told otherwise.
following = os.environ.get("DIRECTOR_START", "on") != "off"
last = {"program": None, "match": None, "error": None}


def log(msg):
    print(time.strftime("%H:%M:%S"), msg, flush=True)


def headers():
    """The mixer's bearer token, when it has one. The quiz gets no header."""
    return {"Authorization": f"Bearer {MIXER_TOKEN}"} if MIXER_TOKEN else {}


def get(url):
    req = urllib.request.Request(url, headers=headers() if url.startswith(MIXER) else {})
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.load(r)


def post(url, body):
    hdrs = {"content-type": "application/json"}
    hdrs.update(headers())
    req = urllib.request.Request(url, data=json.dumps(body).encode(), method="POST", headers=hdrs)
    with urllib.request.urlopen(req, timeout=10) as r:
        return r.status


def delete(url):
    req = urllib.request.Request(url, method="DELETE", headers=headers())
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status
    except urllib.error.HTTPError as e:
        return e.code


def championships():
    """Ids of the championships to watch: the configured one, else all."""
    if CHAMP:
        return [CHAMP]
    data = get(f"{QUIZ}/api/championships")
    items = data if isinstance(data, list) else data.get("championships") or data.get("items") or []
    return [c["id"] for c in items if isinstance(c, dict) and "id" in c]


def live_match():
    """(championship id, match) for the live match started most recently, or (cid, None)."""
    best = (None, None)
    first_cid = None
    for cid in championships():
        first_cid = first_cid or cid
        try:
            d = get(f"{QUIZ}/api/championships/{cid}")
        except Exception as e:
            log(f"championship {cid}: {e}")
            continue
        for m in d.get("matches", []):
            if m.get("status") == "live":
                if best[1] is None or (m.get("startedAt") or "") > (best[1].get("startedAt") or ""):
                    best = (cid, m)
    return best if best[1] else (first_cid, None)


def mixer_status():
    return get(f"{MIXER}/api/status")


def ensure_source(status, sid, name, uri, current_uri_by_id):
    """The mixer source `sid` points at `uri`. Re-add it when the address changes."""
    have = next((s for s in status["sources"] if s["id"] == sid), None)
    if have is not None and current_uri_by_id.get(sid) == uri:
        return have
    if have is not None:
        delete(f"{MIXER}/api/sources/{sid}")
        time.sleep(1)
    post(f"{MIXER}/api/sources", {"id": sid, "name": name, "uri": uri, "kind": "web", "superimpose": "off"})
    current_uri_by_id[sid] = uri
    log(f"source {sid} -> {uri}")
    return None


class Control(BaseHTTPRequestHandler):
    """GET /state, POST /on, POST /off. What the quiz's Go Live button talks to."""

    def _send(self, code, body):
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path.startswith("/state"):
            self._send(200, {"following": following, "program": last["program"], "match": last["match"],
                             "error": last["error"], "stream_url": STREAM_URL})
        else:
            self._send(404, {"error": "no such path"})

    def do_POST(self):
        global following
        if self.path.startswith("/on"):
            following = True
            log("switched on by the control endpoint")
            self._send(200, {"following": True})
        elif self.path.startswith("/off"):
            following = False
            try:
                post(f"{MIXER}/api/take", {"source": None})
                last["program"] = None
                log("switched off: programme cut to black")
            except Exception as e:
                log(f"off: could not cut the programme: {e}")
            self._send(200, {"following": False})
        else:
            self._send(404, {"error": "no such path"})

    def log_message(self, *_):
        pass


def serve_control():
    host, port = BIND.rsplit(":", 1)
    ThreadingHTTPServer((host, int(port)), Control).serve_forever()


def main():
    threading.Thread(target=serve_control, daemon=True).start()
    log(f"control endpoint on {BIND}, following={'on' if following else 'off'}")
    uris = {}
    wanted = None
    while True:
        try:
            if not following:
                time.sleep(PERIOD)
                continue
            cid, match = live_match()
            status = mixer_status()
            last["program"] = status.get("program")
            last["match"] = match["id"] if match else None
            last["error"] = None
            if match:
                # broadcast=1: the watch page starts with sound and hides its
                # Enable Sound control; it knows nobody sits at this browser.
                uri = f"{QUIZ}/watch/{match['id']}?broadcast=1"
                src = ensure_source(status, "match", "Live match", uri, uris)
                target = "match"
            else:
                uri = f"{QUIZ}/championships/{cid}" if cid else None
                src = ensure_source(status, "quiz-champ", "Championship page", uri, uris) if uri else None
                target = "quiz-champ" if uri else None
            if target and src is not None and src.get("state") == "live" and status.get("program") != target:
                post(f"{MIXER}/api/take", {"source": target})
                log(f"programme -> {target} ({uri})")
            elif target and target != wanted:
                log(f"waiting for {target} to come up ({uri})")
            wanted = target
        except Exception as e:
            last["error"] = str(e)
            log(f"error: {e}")
        time.sleep(PERIOD)


if __name__ == "__main__":
    main()
