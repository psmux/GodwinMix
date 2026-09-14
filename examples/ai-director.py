#!/usr/bin/env python3
"""An AI director for GodwinMix.

Every few seconds this reads the mixer's compact state (`agent.state`), decides
whether it needs to see the picture, and if so fetches the mosaic of every
source and the programme. It sends the state, the picture when it has one, and
the programme's goal to a Claude model and asks for one JSON decision:
{"take": "<source id>" or null, "reason": "..."}. When the decision names a
source that is live, differs from what is on programme, and the current shot
has been held long enough, it calls `program.take`. Everything the model says
is checked here before it reaches the mixer.

The picture is fetched on a slow timer (every --look-every cycles) and early
whenever a source's motion number moves by more than --motion-delta since the
last reading, so most cycles cost a few hundred bytes of JSON and no image.

Run it:

    pip install godwinmix anthropic
    export ANTHROPIC_API_KEY=...          # or log in with `ant auth login`
    python3 examples/ai-director.py --url http://HOST:8080 --token TOKEN \\
        "Show the source with a person speaking. Cut to the scoreboard when the
         score changes, hold it ten seconds, then go back."

    --dry-run logs the decisions and takes nothing.
    ANTHROPIC_MODEL picks the model; the default is claude-sonnet-5.

The decision loop is the one in docs/agents.md, and the reasoning behind each
rule is there. The mixer half of it is `godwinmix`: one connection, typed
calls, one error shape.
"""

import argparse
import asyncio
import base64
import json
import os
import re
import sys
import time

import godwinmix

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

JSON_OBJECT = re.compile(r"\{.*\}", re.DOTALL)


def log(message):
    print(time.strftime("%H:%M:%S"), message, flush=True)


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
        match = JSON_OBJECT.search(text)
        if not match:
            raise ValueError("no JSON object in the reply")
        obj = json.loads(match.group(0))
    if not isinstance(obj, dict) or "take" not in obj:
        raise ValueError('reply is JSON but has no "take" field')
    take = obj["take"]
    if take is not None and not isinstance(take, str):
        raise ValueError('"take" must be a string or null')
    reason = obj.get("reason")
    return take, reason if isinstance(reason, str) else ""


def ask_model(client, model, goal, state, image=None):
    """One decision from the model. Returns the raw text of its reply."""
    content = []
    if image is not None:
        content.append({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/jpeg",
                "data": base64.standard_b64encode(image).decode("ascii"),
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


def motion_changed(previous, current, delta):
    """True when any source's motion moved by more than `delta` between readings."""
    if previous is None:
        return True
    before = {s["id"]: s.get("motion") or 0.0 for s in previous.get("sources", [])}
    return any(
        abs((s.get("motion") or 0.0) - before.get(s["id"], 0.0)) > delta
        for s in current.get("sources", [])
    )


def summarise(state):
    parts = []
    for source in state.get("sources", []):
        flag = "*" if source["id"] == state.get("program") else " "
        parts.append(f"{flag}{source['id']}:{source.get('state')}/{(source.get('motion') or 0.0):.2f}")
    return " ".join(parts)


class Director:
    """The loop from docs/agents.md: read, look when it must, decide, take."""

    def __init__(self, mixer, model_client, args):
        self.mixer = mixer
        self.model = model_client
        self.args = args
        self.previous = None
        self.cycles_since_look = 0
        self.last_take_at = 0.0

    async def cycle(self):
        state = await self.mixer.agent_state()
        live = {s["id"] for s in state.get("sources", []) if s.get("state") == "live"}
        program = state.get("program")
        on_air = next((s for s in state.get("sources", []) if s["id"] == program), None)

        # Look at the picture on a slow timer, or as soon as something moves.
        self.cycles_since_look += 1
        image = None
        if self.cycles_since_look >= self.args.look_every or motion_changed(
            self.previous, state, self.args.motion_delta
        ):
            image = await self.look()
        self.previous = state

        # Hold the shot unless what is on air has gone away.
        held = time.monotonic() - self.last_take_at
        if held < self.args.min_hold and on_air is not None and on_air.get("state") == "live":
            log(f"holding {program} ({held:.0f}s of {self.args.min_hold:.0f}s)  {summarise(state)}")
            return

        reply = await asyncio.get_running_loop().run_in_executor(
            None, ask_model, self.model, self.args.model, self.args.goal, state, image
        )
        try:
            take, reason = parse_decision(reply)
        except (ValueError, json.JSONDecodeError) as e:
            # A missed decision is recoverable; a bad take is on air.
            log(f"model reply was not a decision ({e}): {reply[:120]!r}")
            return

        looked = "looked" if image is not None else "numbers"
        if take is None or take == program:
            log(f"keep {program} [{looked}] {reason}  {summarise(state)}")
        elif take not in live:
            known = any(s["id"] == take for s in state.get("sources", []))
            log(f"refused take {take}: {'is not live' if known else 'is not a source'} [{looked}] {reason}")
        elif self.args.dry_run:
            log(f"would take {take} [{looked}] {reason}")
        else:
            await self.take(take, looked, reason)

    async def look(self):
        """The mosaic, as bytes. A failure costs this cycle its picture, nothing more."""
        try:
            # `snapshot.get` answers {mime, bytes, base64}: the same JPEG the
            # REST route serves raw, wrapped so it fits in a JSON-RPC result.
            sheet = await self.mixer.snapshot_get(id="sheet", width=self.args.width)
            self.cycles_since_look = 0
            return base64.standard_b64decode(sheet["base64"])
        except godwinmix.RpcError as e:
            log(f"sheet: {e.message}; deciding from the numbers alone")
            return None
        except (KeyError, ValueError) as e:
            log(f"sheet: the core answered something unreadable ({e})")
            return None

    async def take(self, source, looked, reason):
        try:
            await self.mixer.take(source)
        except godwinmix.RpcError as e:
            log(f"take {source} refused: {e.message}")
            return
        self.last_take_at = time.monotonic()
        log(f"take {source} [{looked}] {reason}")


async def main():
    ap = argparse.ArgumentParser(
        description="Direct a GodwinMix programme with a Claude model.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__.split("Run it:")[0].strip(),
    )
    ap.add_argument("goal", help="what the programme should show, in plain words")
    ap.add_argument("--url", default=os.environ.get("GMX_URL", "http://127.0.0.1:8080"),
                    help="the mixer's control address (default: $GMX_URL or http://127.0.0.1:8080)")
    ap.add_argument("--token", default=os.environ.get("GMX_TOKEN"),
                    help="bearer token for the mixer (default: $GMX_TOKEN)")
    ap.add_argument("--interval", type=float, default=2.0, help="seconds between cycles (default 2)")
    ap.add_argument("--look-every", type=int, default=5,
                    help="fetch the sheet every this many cycles when nothing moves (default 5)")
    ap.add_argument("--motion-delta", type=float, default=0.15,
                    help="a source's motion changing by more than this looks at once (default 0.15)")
    ap.add_argument("--min-hold", type=float, default=8.0,
                    help="seconds a shot is held before another take is allowed (default 8)")
    ap.add_argument("--width", type=int, default=1280, help="width of the sheet sent to the model (default 1280)")
    ap.add_argument("--model", default=DEFAULT_MODEL, help="model id (default: $ANTHROPIC_MODEL or claude-sonnet-5)")
    ap.add_argument("--dry-run", action="store_true", help="decide and log, take nothing")
    args = ap.parse_args()

    if args.interval < 1.0:
        log("interval clamped to 1 second; nothing in the state changes faster than the frame rate")
        args.interval = 1.0

    try:
        import anthropic
    except ImportError:
        sys.exit("the anthropic package is not installed: pip install anthropic")

    try:
        mixer = await godwinmix.connect(args.url, args.token)
    except OSError as e:
        sys.exit(f"cannot reach the mixer at {args.url}: {e}")

    # A director watches nothing: it polls `agent.state` on its own tempo, which
    # is what docs/agents.md recommends and what costs the core least.
    director = Director(mixer, anthropic.Anthropic(), args)
    log(f"directing {args.url} with {args.model}" + (" (dry run)" if args.dry_run else ""))
    log(f"goal: {args.goal}")

    while True:
        started = time.monotonic()
        try:
            await director.cycle()
        except godwinmix.ConnectionClosed as e:
            log(f"{e.message}; reconnecting")
            await asyncio.sleep(args.interval)
            try:
                mixer = await godwinmix.connect(args.url, args.token)
                director.mixer = mixer
                log("mixer is back")
            except OSError:
                continue
        except godwinmix.RpcError as e:
            if e.code == godwinmix.CODES["NO_SCOPE"]:
                sys.exit(f"the mixer refused the token: {e.message}")
            log(f"state: {e.message}")
        await asyncio.sleep(max(0.0, args.interval - (time.monotonic() - started)))


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print()
