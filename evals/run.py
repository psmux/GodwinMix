#!/usr/bin/env python3
"""Run the operator eval suite: 09 section 5 item 20.

Grade the world, not the reply. Every case starts a real core, hands an agent
one instruction, and then looks at what the session log says changed. Whether
the agent said the right thing is not measured, because an agent that says "I
have put the wide camera on air" and does not is the failure this catches.

    evals/run.py --driver scripted            what CI runs
    evals/run.py --driver scripted --case take-the-wide-shot
    evals/run.py --driver claude --model claude-opus-4-6

Each case runs three times and is reported as pass^3: three from three, or it
did not pass. Home Assistant's numbers are why. A frontier model scored 94.6
percent answering questions about devices and 18.5 percent driving them, and a
suite that ran each case once would not have been able to tell those apart.

Standard library only. `clients/python` is on the path for a driver that wants
the first party client; the harness itself speaks REST with urllib so that it
needs no event loop.
"""

import argparse
import datetime
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent


def read_version(manifest):
    """The version out of a gmx-plugin.toml, without a TOML parser.

    Same reason tools/marketplace.py does it this way: Python 3.11 has
    tomllib and 3.9 does not, and this runs on whatever CI has.
    """
    text = manifest.read_text(encoding="utf-8").split("[[provides]]", 1)[0]
    found = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    return found.group(1) if found else "installed"
CASES = pathlib.Path(__file__).resolve().parent / "cases"
RESULTS = pathlib.Path(__file__).resolve().parent / "results"
sys.path.insert(0, str(ROOT / "clients" / "python"))

RUNS_PER_CASE = 3


def rest_route(method):
    """Where a method lives on `/api/v1`, from the committed protocol."""
    global _ROUTES
    if _ROUTES is None:
        document = json.loads((ROOT / "protocol.json").read_text())
        _ROUTES = {m["name"]: m["rest"] for m in document["methods"] if m.get("rest")}
    route = _ROUTES.get(method)
    if route is None:
        raise SystemExit(f"there is no method `{method}` with a REST route in protocol.json")
    return route


_ROUTES = None


# ---------------------------------------------------------------------------
# A core, started and driven the way dev/smoke.sh starts one
# ---------------------------------------------------------------------------


def find_binary(name):
    """The built binary, release first because it is what a bench runs."""
    if os.environ.get("GMX_BIN"):
        return pathlib.Path(os.environ["GMX_BIN"]).parent / name
    for profile in ("release", "debug"):
        candidate = ROOT / "target" / profile / name
        if candidate.exists():
            return candidate
    raise SystemExit(
        f"no {name} binary. Run `cargo build --release` first, or set GMX_BIN."
    )


def free_port():
    import socket

    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


CONFIG = """\
[canvas]
width = 640
height = 360
fps = 30
sample_rate = 48000
channels = 2

[control]
bind = "127.0.0.1:{port}"
token = "{token}"

[multiview]
enabled = false

[safety]
min_hold_ms = {min_hold_ms}
flash_guard = false
"""


class Core:
    """One running mixer, its session log, and the calls a driver makes."""

    def __init__(self, case):
        self.case = case
        self.port = free_port()
        self.token = "eval-token"
        self.base = f"http://127.0.0.1:{self.port}/api/v1"
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="gmx-eval-"))
        self.process = None
        self.mark = 0
        self.baseline = None

    def __enter__(self):
        config = self.dir / "godwinmix.toml"
        initial = self.case.get("initial", {})
        extra = initial.get("config", "")
        wanted = initial.get("plugins", [])
        if wanted:
            extra = self.install_plugins(wanted) + extra
        config.write_text(
            CONFIG.format(
                port=self.port,
                token=self.token,
                min_hold_ms=initial.get("min_hold_ms", 0),
            )
            + extra
        )
        self.process = subprocess.Popen(
            [str(find_binary("godwinmix")), "--config", str(config)],
            cwd=self.dir,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(200):
            try:
                self.get("core/info")
                break
            except Exception:
                if self.process.poll() is not None:
                    raise SystemExit("the core exited before it answered")
                time.sleep(0.1)
        else:
            raise SystemExit("the core never came up")
        self.set_up()
        return self

    def __exit__(self, *exc):
        """Stop it, and be sure it is stopped.

        A suite that leaves a mixer behind on every case ends with thirty of
        them competing for the machine, and the timings it then measures are
        of the machine rather than of the mixer.
        """
        import shutil
        import signal

        try:
            self.post("core/shutdown", {})
        except Exception:
            pass
        # Graceful, then not. A suite that leaves a mixer behind on every case
        # ends with thirty of them competing for the machine, and by then the
        # numbers it reports are of the machine and not of the mixer.
        for attempt in (signal.SIGTERM, signal.SIGKILL, signal.SIGKILL):
            try:
                self.process.wait(timeout=5)
                break
            except Exception:
                try:
                    self.process.send_signal(attempt)
                except Exception:
                    break
        if self.process.poll() is None:
            print(f"    {self.case['id']}: the core would not stop (pid {self.process.pid})")
        shutil.rmtree(self.dir, ignore_errors=True)

    # -- the protocol --------------------------------------------------

    def _request(self, path, data=None, method=None):
        body = None if data is None else json.dumps(data).encode()
        request = urllib.request.Request(
            f"{self.base}/{path}",
            data=body,
            method=method or ("POST" if data is not None else "GET"),
            headers={
                "authorization": f"Bearer {self.token}",
                "content-type": "application/json",
            },
        )
        with urllib.request.urlopen(request, timeout=20) as answer:
            text = answer.read().decode()
        return json.loads(text) if text.strip() else {}

    def get(self, path):
        return self._request(path)

    def post(self, path, data):
        return self._request(path, data)

    def delete(self, path):
        return self._request(path, None, method="DELETE")

    def call(self, method, params=None):
        """One protocol call, by its method name, as a driver makes it.

        The path and the verb come from `protocol.json`, not from a table kept
        here. One protocol: a harness with its own idea of where `source.add`
        lives is a harness that stops testing the thing it claims to.
        """
        rest = rest_route(method)
        params = dict(params or {})
        path = rest["path"].removeprefix("/api/v1/")
        for field in re.findall(r"\{(\w+)\}", path):
            value = params.get(field)
            if value is None:
                raise SystemExit(f"{method} needs `{field}` and was not given one")
            path = path.replace("{" + field + "}", str(value))
        verb = rest["method"]
        if verb == "GET":
            return self.get(path)
        return self._request(path, params, method=verb)

    # -- the state a case starts in ------------------------------------

    def install_plugins(self, names):
        """Stage the named plugins into this run's own plugins directory.

        Copied rather than pointed at, because the loader expects
        `<dir>/<name>/<version>/` and the repository keeps a plugin at
        `plugins/<name>/`. A copy also means an eval cannot write into the
        working tree.
        """
        root = ROOT / "plugins"
        target = self.dir / "plugins"
        for name in names:
            source = root / name
            if not (source / "gmx-plugin.toml").is_file():
                raise SystemExit(f"there is no plugin at {source}")
            version = read_version(source / "gmx-plugin.toml")
            shutil.copytree(source, target / name / version)
        return f'[server]\nplugins_dir = "{target}"\n\n'

    def check_plugins(self, names):
        """Every plugin the case asked for is loaded and has no problem."""
        loaded = {p["name"]: p for p in self.get("plugins").get("plugins", [])}
        for name in names:
            plugin = loaded.get(name)
            if plugin is None:
                raise SystemExit(
                    f"the case wants the `{name}` plugin and the core did not load it. "
                    f"Loaded: {', '.join(loaded) or 'none'}."
                )
            if plugin.get("problem"):
                raise SystemExit(f"`{name}` did not load: {plugin['problem']}")
            if not plugin.get("instances"):
                raise SystemExit(
                    f"`{name}` is installed and nothing is running it. A plugin whose only "
                    f"placement is `wasm` needs a core built with `--features wasm`; "
                    f"`gmx doctor` says whether this one is."
                )

    def set_up(self):
        """The world as the case starts, before the instruction arrives.

        None of this is graded: the mark is taken at the end, so the deltas a
        case is judged on are only the ones the agent caused.
        """
        initial = self.case.get("initial", {})
        if initial.get("plugins"):
            self.check_plugins(initial["plugins"])
        for source in initial.get("sources", []):
            self.call("source.add", source)
        for source in initial.get("sources", []):
            self.wait_for(source["id"], "live")
        # The listing says live before the event saying so has been written
        # down, and a mark taken in between would put "cam-close went live" on
        # the agent's account. Wait for the log, not for the listing.
        self.wait_for_log([s["id"] for s in initial.get("sources", [])])
        if initial.get("gain"):
            # Somewhere that is not unity, so "back to 0 dB" is a change.
            self.call("source.audio.set", {"id": "cam-floor", "gain": 10 ** (-12 / 20)})
        if initial.get("mute"):
            self.call("source.audio.set", {"id": initial["mute"], "muted": True})
        # Setting the world up is not the thing being measured, so a refusal
        # here is reported and stepped over rather than raised: a case whose
        # own safety settings refuse part of its own setup is a case to fix,
        # and a traceback in the middle of a suite hides which one it was.
        for key in ("program", "program_twice"):
            if not initial.get(key):
                continue
            try:
                self.call("program.take", {"source": initial[key]})
            except Exception as e:
                print(f"    setting up {self.case['id']}: {key} was refused ({e})")
            time.sleep(0.4)
        if initial.get("adbreak"):
            self.call("adbreak.start", {"uri": clip()})
            time.sleep(1.5)
        # A case that wants a hold to have run out before the instruction
        # arrives says how long to wait for it.
        if initial.get("wait_ms"):
            time.sleep(initial["wait_ms"] / 1000)
        self.settle()
        self.mark = self.log_size()
        self.baseline = self.status_event()

    def settle(self, quiet=0.5, limit=10):
        """Wait until the log stops growing.

        A REST call answers before the event it caused has been written down, so
        a mark taken the moment setting up returns would put the tail of the
        setup on the agent's account. Waiting for quiet is the only honest way
        to draw the line.
        """
        deadline = time.time() + limit
        last = -1
        while time.time() < deadline:
            now = self.log_size()
            if now == last:
                return
            last = now
            time.sleep(quiet)

    def status_event(self):
        """The whole status, shaped like the event that carries it.

        The mixer publishes a status when the shape of the show changes, not
        when a source finishes connecting, so the log on its own can be a few
        seconds behind the world. Reading the status at the mark and again at
        the end pins both ends of what the agent is answerable for.
        """
        return dict(self.get("core/status"), type="status")

    def wait_for_log(self, ids, seconds=15):
        """Wait until the log has said every source is live."""
        deadline = time.time() + seconds
        while time.time() < deadline:
            live = {
                d["id"]
                for d in deltas_of(self.records())
                if d["what"] == "source" and d["to"] == "live"
            }
            if all(source_id in live for source_id in ids):
                return
            time.sleep(0.2)

    def wait_for(self, source_id, state, seconds=15):
        deadline = time.time() + seconds
        while time.time() < deadline:
            for source in self.get("sources"):
                if source["id"] == source_id and source["state"] == state:
                    return True
            time.sleep(0.2)
        return False

    # -- the session log -----------------------------------------------

    @property
    def log_path(self):
        return self.dir / ".godwinmix" / "session.jsonl"

    def log_size(self):
        """How many records the log holds. The mark is a count, not an offset.

        The tracker has to see the records before the mark as well, or the
        first status after it would read as every source changing state at
        once. It sees them and says nothing about them.
        """
        return len(self.records())

    def records(self):
        if not self.log_path.exists():
            return []
        out = []
        for line in self.log_path.read_text(errors="replace").splitlines():
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                continue
        return out


def clip():
    """The two second clip `gmx session replay` makes, for the ad cases."""
    path = pathlib.Path(tempfile.gettempdir()) / "gmx-replay-ad.mkv"
    if not path.exists():
        subprocess.run(
            [
                "gst-launch-1.0", "-q",
                "videotestsrc", "num-buffers=60", "pattern=smpte", "!",
                "video/x-raw,width=320,height=180,framerate=30/1", "!",
                "videoconvert", "!", "theoraenc", "!", "matroskamux", "name=m", "!",
                "filesink", f"location={path}",
                "audiotestsrc", "num-buffers=94", "!", "audioconvert", "!",
                "vorbisenc", "!", "m.",
            ],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    return str(path)


# ---------------------------------------------------------------------------
# State deltas: the same vocabulary `gmx session replay` grades on
# ---------------------------------------------------------------------------

# Commands whose effect the status document does not carry, so the command
# record is the only place the value shows up. Kept deliberately short: a
# grader that reads commands is grading what was asked for rather than what
# happened, and that is the thing this suite exists not to do.
PARAM_FIELDS = {
    "program.take": ["at_running_time_ms"],
    "filter.add": ["params"],
    "filter.set": ["params"],
}


def db_of(linear):
    """A linear gain as decibels, to one place. 0.501 is -6.0 dB."""
    import math

    if linear is None:
        return None
    if linear <= 0:
        return "-inf"
    return f"{20 * math.log10(linear):.1f}"


class Tracker:
    """Session log records in, state deltas out."""

    def __init__(self):
        self.sources = {}
        self.outputs = {}
        self.gains = {}
        self.mutes = {}
        self.ad = None

    def absorb(self, record):
        kind = record.get("kind")
        if kind == "command":
            return self.command(record)
        if kind != "event":
            return []
        return self.event(record.get("event") or {})

    def command(self, record):
        method = record.get("method", "")
        out = []
        for field in PARAM_FIELDS.get(method, []):
            value = (record.get("params") or {}).get(field)
            if value is None:
                continue
            shown = json.dumps(value, sort_keys=True) if isinstance(value, (dict, list)) else str(value)
            out.append({"what": "param", "id": f"{method}.{field}", "to": shown})
        return out

    def event(self, event):
        kind = event.get("type")
        if kind == "took":
            return [{"what": "program", "to": event.get("source") or "slate"}]
        if kind == "source_state_changed":
            return self.source(event.get("source"), event.get("state"))
        if kind == "output_state_changed":
            return self.output(event.get("output"), event.get("state"))
        if kind == "ad_break_changed":
            return self.adbreak(event.get("ad"))
        if kind == "hook_blocked":
            return [{"what": "hook", "id": event.get("hook", ""), "to": "blocked"}]
        if kind != "status":
            return []
        out = []
        seen = set()
        for source in event.get("sources") or []:
            seen.add(source["id"])
            out += self.source(source["id"], source.get("state"))
            out += self.audio(source)
        for gone in [s for s in self.sources if s not in seen]:
            del self.sources[gone]
            out.append({"what": "source", "id": gone, "to": "removed"})
        for output in event.get("outputs") or []:
            out += self.output(output.get("id"), output.get("state"))
        out += self.adbreak(event.get("ad"))
        return out

    def source(self, source_id, state):
        if not source_id or self.sources.get(source_id) == state:
            return []
        self.sources[source_id] = state
        return [{"what": "source", "id": source_id, "to": state}]

    def output(self, output_id, state):
        if not output_id or self.outputs.get(output_id) == state:
            return []
        self.outputs[output_id] = state
        return [{"what": "output", "id": output_id, "to": state}]

    def audio(self, source):
        out = []
        source_id = source["id"]
        gain = db_of(source.get("gain"))
        if gain is not None and self.gains.get(source_id) != gain:
            # Not on the first sight of a source: its starting gain is the
            # world before the instruction, not a change the agent made.
            if source_id in self.gains:
                out.append({"what": "gain", "id": source_id, "to": f"{gain} dB"})
            self.gains[source_id] = gain
        muted = "muted" if source.get("muted") else "unmuted"
        if self.mutes.get(source_id) != muted:
            if source_id in self.mutes:
                out.append({"what": "mute", "id": source_id, "to": muted})
            self.mutes[source_id] = muted
        return out

    def adbreak(self, ad):
        to = None if not ad else ("on air" if ad.get("on_air") else "armed")
        if to is None and self.ad is None:
            return []
        if to == self.ad:
            return []
        self.ad = to
        return [{"what": "adbreak", "to": to or "ended"}]


def deltas_of(records, after=0, baseline=None, final=None):
    """Deltas from the records at or past `after`.

    The records before it are read for context and produce nothing: the world
    before the instruction is not the agent's doing.

    `baseline` is the status as it actually was at the mark, and `final` the
    status at the end. Both are read from the core rather than from the log,
    because the mixer publishes a status when the shape of the show changes
    and not when a source finishes connecting. Without them a source that went
    live a moment before the mark is reported as the agent's doing, and one
    that went live a moment after it is not reported at all.
    """
    tracker = Tracker()
    for record in records[:after]:
        tracker.absorb(record)
    if baseline is not None:
        tracker.event(baseline)
    out = []
    for record in records[after:]:
        out += tracker.absorb(record)
    if final is not None:
        out += tracker.event(final)
    return out


def subject(delta):
    return (delta.get("what", ""), delta.get("id", ""))


def grade(expected, produced):
    """What differs, by subject.

    What one source did is in order; what two sources did relative to each
    other is not, because they are two pipelines on two threads. The same rule
    `gmx session diff` uses, for the same reason.
    """
    lines = []
    subjects = []
    for delta in list(expected) + list(produced):
        if subject(delta) not in subjects:
            subjects.append(subject(delta))
    for key in subjects:
        want = [d.get("to", "") for d in expected if subject(d) == key]
        got = [d.get("to", "") for d in produced if subject(d) == key]
        # A scheduled take carries a running time that differs on every run.
        # What is being graded is that one was given at all, not which.
        want = [g if w == "SCHEDULED" else w for w, g in zip(want, got)] + want[len(got):]
        if want != got:
            name = key[0] if not key[1] else f"{key[0]} {key[1]}"
            lines.append(f"{name}: expected [{', '.join(want)}], got [{', '.join(got)}]")
    return lines


# ---------------------------------------------------------------------------
# Drivers
# ---------------------------------------------------------------------------

from drivers import scripted as scripted_driver  # noqa: E402
from drivers import cli as cli_driver  # noqa: E402

DRIVERS = {
    "scripted": scripted_driver.drive,
    "claude": lambda core, case, options: cli_driver.drive("claude", core, case, options),
    "codex": lambda core, case, options: cli_driver.drive("codex", core, case, options),
}


# ---------------------------------------------------------------------------
# Running
# ---------------------------------------------------------------------------


def load_cases(only=None, tag=None):
    cases = []
    for path in sorted(CASES.glob("*.json")):
        case = json.loads(path.read_text())
        case.setdefault("id", path.stem)
        if only and case["id"] != only:
            continue
        if tag and tag not in case.get("tags", []):
            continue
        cases.append(case)
    if not cases:
        raise SystemExit("no cases matched")
    return cases


def run_once(case, driver, options):
    """One run of one case: start a core, give the instruction, look."""
    started = time.time()
    with Core(case) as core:
        report = driver(core, case, options) or {}
        # The take lands on the next frame boundary and a source goes live
        # when it produces one, so the world is read a moment later. A case
        # that arms something for later says how much later.
        time.sleep(case.get("settle_ms", 1200) / 1000)
        core.settle()
        produced = deltas_of(
            core.records(),
            after=core.mark,
            baseline=core.baseline,
            final=core.status_event(),
        )
    problems = grade(case.get("expect_changes", []), produced)
    return {
        "passed": not problems,
        "problems": problems,
        "produced": produced,
        "seconds": round(time.time() - started, 1),
        "cost_usd": report.get("cost_usd"),
        "tokens": report.get("tokens"),
        "tool_calls": report.get("tool_calls", []),
    }


def run_case(case, driver, options):
    if case.get("pending"):
        return {"id": case["id"], "pending": case["pending"], "tags": case.get("tags", [])}
    runs = [run_once(case, driver, options) for _ in range(options.runs)]
    passed = sum(1 for r in runs if r["passed"])
    return {
        "id": case["id"],
        "tags": case.get("tags", []),
        "instruction": case["instruction"],
        "runs": runs,
        "passed": passed,
        "pass_cubed": passed == options.runs,
        "cost_usd": sum(r["cost_usd"] or 0 for r in runs) or None,
        "tokens": sum(r["tokens"] or 0 for r in runs) or None,
    }


def report(results, options):
    """`evals/results/<date>.md`, and the same table on stdout."""
    done = [r for r in results if "runs" in r]
    pending = [r for r in results if "pending" in r]
    cubed = [r for r in done if r["pass_cubed"]]
    lines = []
    lines.append(f"# Operator eval, {datetime.date.today().isoformat()}")
    lines.append("")
    lines.append(f"Driver `{options.driver}`" + (f", model `{options.model}`" if options.model else ""))
    lines.append("")
    lines.append(
        f"**{len(cubed)} of {len(done)} at pass^3** ({options.runs} runs each). "
        f"{len(pending)} case(s) pending."
    )
    lines.append("")
    total_cost = sum(r["cost_usd"] or 0 for r in done)
    total_tokens = sum(r["tokens"] or 0 for r in done)
    if total_cost or total_tokens:
        lines.append(f"Cost ${total_cost:.4f}, {total_tokens} tokens.")
        lines.append("")
    for name, tag in [
        ("Abstain", "abstain"),
        ("Parameterised", "parameterised"),
        ("Interruption", "interruption"),
    ]:
        group = [r for r in done if tag in r["tags"]]
        if group:
            passing = sum(1 for r in group if r["pass_cubed"])
            lines.append(f"* {name}: {passing} of {len(group)} at pass^3.")
    lines.append("")
    lines.append("| Case | Tags | pass^3 | Runs | Cost | Tokens | What differed |")
    lines.append("|---|---|---|---|---|---|---|")
    for r in done:
        cost = f"${r['cost_usd']:.4f}" if r["cost_usd"] else ""
        tokens = str(r["tokens"]) if r["tokens"] else ""
        why = ""
        for run in r["runs"]:
            if run["problems"]:
                why = "; ".join(run["problems"])[:160]
                break
        lines.append(
            f"| `{r['id']}` | {', '.join(r['tags'])} | {'yes' if r['pass_cubed'] else 'NO'} "
            f"| {r['passed']}/{options.runs} | {cost} | {tokens} | {why} |"
        )
    for r in pending:
        lines.append(f"| `{r['id']}` | {', '.join(r['tags'])} | pending | | | | {r['pending']} |")
    lines.append("")
    text = "\n".join(lines) + "\n"
    RESULTS.mkdir(parents=True, exist_ok=True)
    out = RESULTS / f"{datetime.date.today().isoformat()}.md"
    out.write_text(text)
    print(text)
    print(f"written to {out.relative_to(ROOT)}")
    return len(cubed) == len(done)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--driver", default="scripted", choices=sorted(DRIVERS))
    parser.add_argument("--case", help="one case id")
    parser.add_argument("--tag", help="only cases with this tag")
    parser.add_argument("--runs", type=int, default=RUNS_PER_CASE)
    parser.add_argument("--model", help="passed to the claude and codex drivers")
    parser.add_argument(
        "--allow-failures",
        action="store_true",
        help="exit 0 even when a case is not at pass^3",
    )
    options = parser.parse_args()
    cases = load_cases(options.case, options.tag)
    driver = DRIVERS[options.driver]
    results = []
    for case in cases:
        results.append(run_case(case, driver, options))
        last = results[-1]
        mark = "pending" if "pending" in last else ("ok" if last["pass_cubed"] else "FAILED")
        print(f"  {last['id']:<40} {mark}", flush=True)
    everything_passed = report(results, options)
    if not everything_passed and not options.allow_failures:
        sys.exit(1)


if __name__ == "__main__":
    main()
