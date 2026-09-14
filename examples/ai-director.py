#!/usr/bin/env python3
"""An AI director for GodwinMix.

Every few seconds this reads the mixer's compact state (GET /api/agent/state),
decides whether it needs to see the picture, and if so fetches the mosaic of
every source and the programme (GET /api/snapshot/sheet.jpg). It sends the
state, the picture when it has one, and the programme's goal to a Claude
model and asks for one JSON decision: {"take": "<source id>" or null,
"reason": "..."}. When the decision names a source that is live, differs from
what is on programme, and the current shot has been held long enough, it
posts POST /api/take. Everything the model says is checked here before it
reaches the mixer.

The picture is fetched on a slow timer (every --look-every cycles) and early
whenever a source's motion number moves by more than --motion-delta since the
last reading, so most cycles cost a few hundred bytes of JSON and no image.

Run it:

    pip install anthropic
    export ANTHROPIC_API_KEY=...          # or log in with `ant auth login`
    python3 examples/ai-director.py --url http://HOST:8080 --token TOKEN \\
        "Show the source with a person speaking. Cut to the scoreboard when the
         score changes, hold it ten seconds, then go back."

    --dry-run logs the decisions and posts nothing.
    ANTHROPIC_MODEL picks the model; the default is claude-sonnet-5.

Python 3.9 or later, the `anthropic` package, and the standard library.
See docs/agents.md for the reasoning behind each rule in here.
"""

import argparse
import base64
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

DEFAULT_MODEL = os.environ.get("ANTHROPIC_MODEL", "claude-sonnet-5")

SYSTEM_PROMPT = """You are the director of a live television programme run on a video mixer.
Several sources are available and exactly one of them is on programme (on air) at a time.
Your job is to decide, once per cycle, which source should be on programme.

The goal of the programme, from the producer:
{goal}

Each cycle you receive the mixer's state as JSON and sometimes a picture. The picture
is a mosaic: every source and the programme output in a grid, each cell labelled with
its source id. The JSON has, for every source: its id and name, its state (only "live"
sources may be taken), whether it has video and audio, video_idle_ms (how long since
its last frame), and motion (0.0 to 1.0, how much its picture changed between its last
two frames; near 0 is static, a talking person is around 0.1 to 0.3, a moving camera
is higher). "program" is the id on air now, or null for black.

Answer with a single JSON object and nothing else, in this exact shape:
{{"take": "<source id>", "reason": "<one short sentence>"}}
Use "take": null to leave the programme as it is. Prefer leaving it alone: a
programme that changes shot every few seconds is unwatchable. Never name a source
whose state is not "live". Keep the reason under twenty words."""


def log(msg):
    print(time.strftime("%H:%M:%S"), msg, flush=True)


class Mixer:
    """A thin client for the parts of the HTTP API a director uses."""

    def __init__(self, url, token=None, timeout=10):
        self.base = url.rstrip("/")
        self.token = token
        self.timeout = timeout

    def _request(self, method, path, body=None, query=None):
        url = self.base + path
        if query:
            url += "?" + urllib.parse.urlencode(query)
        data = None
        headers = {}
        if self.token:
            headers["Authorization"] = "Bearer " + self.token
        if body is not None:
            data = json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(url, data=data, method=method, headers=headers)
        with urllib.request.urlopen(req, timeout=self.timeout) as resp:
            return resp.read(), resp.headers.get("Content-Type", "")

    def state(self):
        raw, _ = self._request("GET", "/api/agent/state")
        return json.loads(raw)

    def sheet(self, width):
        raw, ctype = self._request("GET", "/api/snapshot/sheet.jpg", query={"width": width})
        return raw, (ctype.split(";")[0].strip() or "image/jpeg")

    def take(self, source):
        self._request("POST", "/api/take", body={"source": source})


class MixerError(Exception):
    pass


def describe_http_error(e):
    """The mixer answers refusals with the reason in the body. Keep it."""
    try:
        body = e.read().decode(errors="replace").strip()
    except Exception:
        body = ""
    return f"{e.code} {e.reason}" + (f": {body}" if body else "")


JSON_OBJECT = re.compile(r"\{.*\}", re.DOTALL)


def parse_decision(text):
    """Pull the decision out of whatever the model wrote.

    Accepts the bare object, an object inside a code fence, or an object with
    prose around it. Returns (take, reason) or raises ValueError.
    """
    text = text.strip()
    if text.startswith("```"):
        text = re.sub(r"^```[a-zA-Z]*\s*|\s*```$", "", text)
    try:
        obj = json.loads(text)
    except json.JSONDecodeError:
        m = JSON_OBJECT.search(text)
        if not m:
            raise ValueError("no JSON object in the reply")
        obj = json.loads(m.group(0))
    if not isinstance(obj, dict) or "take" not in obj:
        raise ValueError("reply is JSON but has no \"take\" field")
    take = obj["take"]
    if take is not None and not isinstance(take, str):
        raise ValueError("\"take\" must be a string or null")
    reason = obj.get("reason")
    if not isinstance(reason, str):
        reason = ""
    return take, reason


def ask_model(client, model, goal, state, image=None):
    """One decision from the model. Returns the raw text of its reply."""
    content = []
    if image is not None:
        data, media_type = image
        content.append({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media_type,
                "data": base64.standard_b64encode(data).decode("ascii"),
            },
        })
        content.append({"type": "text", "text": "The mosaic above is the current picture of every source and the programme."})
    content.append({"type": "text", "text": "Mixer state:\n" + json.dumps(state, indent=1)})
    content.append({"type": "text", "text": "Decide. Reply with the JSON object only."})

    response = client.messages.create(
        model=model,
        max_tokens=256,
        system=SYSTEM_PROMPT.format(goal=goal),
        messages=[{"role": "user", "content": content}],
    )
    if response.stop_reason == "refusal":
        raise RuntimeError("the model declined to answer")
    return "".join(block.text for block in response.content if block.type == "text")


def motion_changed(prev, cur, delta):
    """True when any source's motion moved by more than `delta` between two readings."""
    if prev is None:
        return True
    before = {s["id"]: s.get("motion") or 0.0 for s in prev.get("sources", [])}
    for s in cur.get("sources", []):
        if abs((s.get("motion") or 0.0) - before.get(s["id"], 0.0)) > delta:
            return True
    return False


def summarise(state):
    parts = []
    for s in state.get("sources", []):
        flag = "*" if s["id"] == state.get("program") else " "
        parts.append(f"{flag}{s['id']}:{s.get('state')}/{(s.get('motion') or 0.0):.2f}")
    return " ".join(parts)


def main():
    ap = argparse.ArgumentParser(
        description="Direct a GodwinMix programme with a Claude model.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__.split("Run it:")[0].strip(),
    )
    ap.add_argument("goal", help="what the programme should show, in plain words")
    ap.add_argument("--url", default=os.environ.get("GODWINMIX_URL", "http://127.0.0.1:8080"),
                    help="the mixer's control address (default: $GODWINMIX_URL or http://127.0.0.1:8080)")
    ap.add_argument("--token", default=os.environ.get("GODWINMIX_TOKEN"),
                    help="bearer token for the mixer (default: $GODWINMIX_TOKEN)")
    ap.add_argument("--interval", type=float, default=2.0,
                    help="seconds between cycles, 1 or more (default 2)")
    ap.add_argument("--look-every", type=int, default=5,
                    help="fetch the sheet every this many cycles when nothing moves (default 5)")
    ap.add_argument("--motion-delta", type=float, default=0.15,
                    help="a source's motion changing by more than this fetches the sheet at once (default 0.15)")
    ap.add_argument("--min-hold", type=float, default=8.0,
                    help="seconds a shot is held before another take is allowed (default 8)")
    ap.add_argument("--width", type=int, default=1280,
                    help="width of the sheet JPEG sent to the model (default 1280)")
    ap.add_argument("--model", default=DEFAULT_MODEL,
                    help="model id (default: $ANTHROPIC_MODEL or claude-sonnet-5)")
    ap.add_argument("--dry-run", action="store_true",
                    help="decide and log, post nothing")
    args = ap.parse_args()

    if args.interval < 1.0:
        log("interval clamped to 1 second; nothing in the state changes faster than the frame rate")
        args.interval = 1.0

    try:
        import anthropic
    except ImportError:
        sys.exit("the anthropic package is not installed: pip install anthropic")
    client = anthropic.Anthropic()

    mixer = Mixer(args.url, args.token)
    log(f"directing {args.url} with {args.model}" + (" (dry run)" if args.dry_run else ""))
    log(f"goal: {args.goal}")

    prev_state = None
    cycles_since_look = 0
    last_take_at = 0.0
    mixer_was_down = False

    while True:
        started = time.monotonic()
        try:
            state = mixer.state()
        except urllib.error.HTTPError as e:
            if e.code in (401, 403):
                sys.exit(f"the mixer refused the token: {describe_http_error(e)}")
            log(f"state: {describe_http_error(e)}")
            time.sleep(args.interval)
            continue
        except (urllib.error.URLError, OSError, ValueError) as e:
            if not mixer_was_down:
                log(f"mixer not answering at {args.url}: {e}")
                mixer_was_down = True
            time.sleep(args.interval)
            continue
        if mixer_was_down:
            log("mixer is back")
            mixer_was_down = False

        program = state.get("program")
        live = {s["id"] for s in state.get("sources", []) if s.get("state") == "live"}
        on_air = next((s for s in state.get("sources", []) if s["id"] == program), None)

        # Look at the picture on a slow timer, or as soon as something moves.
        cycles_since_look += 1
        want_look = cycles_since_look >= args.look_every or motion_changed(prev_state, state, args.motion_delta)
        image = None
        if want_look:
            try:
                image = mixer.sheet(args.width)
                cycles_since_look = 0
            except (urllib.error.URLError, OSError) as e:
                log(f"sheet: {e}; deciding from the numbers alone")
        prev_state = state

        # Hold the shot unless the source on air has gone away.
        held_for = time.monotonic() - last_take_at
        shot_ok = on_air is not None and on_air.get("state") == "live"
        if held_for < args.min_hold and shot_ok:
            log(f"holding {program} ({held_for:.0f}s of {args.min_hold:.0f}s)  {summarise(state)}")
            time.sleep(max(0.0, args.interval - (time.monotonic() - started)))
            continue

        try:
            reply = ask_model(client, args.model, args.goal, state, image)
        except anthropic.RateLimitError:
            log("rate limited by the API; waiting a cycle")
            time.sleep(args.interval)
            continue
        except anthropic.APIConnectionError as e:
            log(f"cannot reach the API: {e}")
            time.sleep(args.interval)
            continue
        except anthropic.APIStatusError as e:
            if e.status_code >= 500:
                log(f"API error {e.status_code}; waiting a cycle")
                time.sleep(args.interval)
                continue
            sys.exit(f"API refused the request: {e.status_code} {e.message}")
        except RuntimeError as e:
            log(str(e))
            time.sleep(args.interval)
            continue

        try:
            take, reason = parse_decision(reply)
        except (ValueError, json.JSONDecodeError) as e:
            log(f"model reply was not a decision ({e}): {reply[:120]!r}")
            time.sleep(max(0.0, args.interval - (time.monotonic() - started)))
            continue

        looked = "looked" if image is not None else "numbers"
        if take is None or take == program:
            log(f"keep {program} [{looked}] {reason}  {summarise(state)}")
        elif take not in live:
            known = any(s["id"] == take for s in state.get("sources", []))
            why = "is not live" if known else "is not a source"
            log(f"refused take {take}: {why} [{looked}] {reason}")
        else:
            if args.dry_run:
                log(f"would take {take} [{looked}] {reason}")
            else:
                try:
                    mixer.take(take)
                    log(f"take {take} [{looked}] {reason}")
                except urllib.error.HTTPError as e:
                    log(f"take {take} refused: {describe_http_error(e)}")
                except (urllib.error.URLError, OSError) as e:
                    log(f"take {take} failed: {e}")
                else:
                    last_take_at = time.monotonic()

        time.sleep(max(0.0, args.interval - (time.monotonic() - started)))


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print()
