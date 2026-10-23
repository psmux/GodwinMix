#!/usr/bin/env python3
"""A headend on this machine, for working on the headend wall.

Starts eight multicast groups of six programs each and two unicast feeds
with GStreamer, a station from target/debug in a folder of its own with the
ingest plugin and the UI read from ui/ on disk, and fifty direct shows on
those feeds with an HLS output each. Prints the page's address and runs
until Ctrl+C, then stops every feed and the station and removes the folder.

Some channels are made to show the states the wall draws:

  * group 8 (channels 143 to 148) has no sender at all: one dead group;
  * group 3 loses a packet in a thousand: continuity errors on six channels;
  * channel 105 is black, 110 frozen, 115 silent and 120 has no sound;
  * channels 125, 130 and 135 carry two sound tracks;
  * channel 150 is a unicast port with no sender, and 149 is stopped.

    python3 dev/headend-wall.py [--port N] [--shows 50]

Needs gst-launch-1.0 with x264enc, avenc_aac and mpegtsmux (and textoverlay
for names on the pictures, when it is there), and
`cargo build -p godwinmix -p gmx-ingest` done first. Standard library only.
"""

import argparse, json, os, shlex, shutil, signal, socket, subprocess, sys, tempfile, time, urllib.error, urllib.request

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(REPO, "target", "debug")
GROUPS, PER_GROUP = 8, 6
PATTERNS = ["smpte", "ball", "bar", "colors", "pinwheel", "snow", "gradient", "spokes", "smpte75", "checkers-8"]


KINDS = ["News", "Sport", "Movies", "Kids", "Music", "Weather", "Docs", "Travel", "Comedy", "Shop"]


def channel(i):
    """Channel i (0 based): its number, a made up name and how it is made."""
    n = 101 + i
    return {"n": n, "name": f"{KINDS[i % len(KINDS)]} {i // len(KINDS) + 1}", "black": n == 105, "frozen": n == 110, "silent": n == 115,
            "mute": n == 120, "two": n in (125, 130, 135), "kbps": 350 + (i * 37) % 600}


def have(element):
    return subprocess.run(["gst-inspect-1.0", element], capture_output=True).returncode == 0


# The channel's number and name on its picture, where pango is installed.
TEXT = have("textoverlay") and have("timeoverlay")


def video(c, pid):
    pat = "black" if c["black"] else PATTERNS[c["n"] % len(PATTERNS)]
    speed = "" if c["frozen"] or c["black"] else " horizontal-speed=2"
    clock = "" if c["frozen"] or not TEXT else " ! timeoverlay halignment=right valignment=bottom font-desc=\"Sans 14\""
    label = "" if c["black"] or not TEXT else f" ! textoverlay text=\"{c['n']}  {c['name']}\" valignment=top halignment=left font-desc=\"Sans Bold 22\""
    return (f"videotestsrc is-live=true pattern={pat}{speed} ! video/x-raw,format=I420,width=320,height=180,framerate=25/1{label}{clock} "
            f"! x264enc tune=zerolatency speed-preset=ultrafast key-int-max=50 bitrate={c['kbps']} ! h264parse ! m.sink_{pid}")


def audio(c, pid, freq):
    wave = "silence" if c["silent"] else "sine"
    return (f"audiotestsrc is-live=true wave={wave} freq={freq} volume=0.3 ! audioconvert ! audioresample "
            f"! audio/x-raw,rate=48000,channels=2 ! avenc_aac bitrate=96000 ! aacparse ! m.sink_{pid}")


def mux(chans, host, port, drop=False):
    """One gst-launch for these channels, program numbers 1 up, to host:port."""
    parts, progs = [], []
    for p, c in enumerate(chans, start=1):
        base = 256 + p * 16
        parts.append(video(c, base))
        progs.append(f"sink_{base}=(int){p}")
        tracks = [] if c["mute"] else [440 + 110 * p] + ([880 + 55 * p] if c["two"] else [])
        for t, freq in enumerate(tracks, start=1):
            parts.append(audio(c, base + t, freq))
            progs.append(f"sink_{base + t}=(int){p}")
    lossy = " ! identity drop-probability=0.001" if drop else ""
    multicast = " auto-multicast=true ttl-mc=0" if host.startswith("239.") else ""
    head = f"mpegtsmux name=m alignment=7 prog-map=\"program_map,{','.join(progs)}\"{lossy} ! udpsink host={host} port={port} sync=false{multicast}"
    return f"{head} " + " ".join(parts)


def gst(line):
    return subprocess.Popen(["gst-launch-1.0", "-q"] + shlex.split(line), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def post(port, path, params):
    """One REST call; answers {"result": ...} or {"error": ...}."""
    req = urllib.request.Request(f"http://127.0.0.1:{port}/api/v1/{path}", json.dumps(params).encode(), {"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return {"result": json.loads(r.read() or b"{}")}
    except urllib.error.HTTPError as e:
        return {"error": e.read().decode(errors="replace")}


def station(folder, port):
    example = subprocess.run([os.path.join(BIN, "godwinmix"), "--example-config"], capture_output=True, text=True).stdout
    cfg = example.replace('bind = "0.0.0.0:8080"', f'bind = "127.0.0.1:{port}"')
    cfg = cfg.replace('# plugins_dir = "~/.godwinmix/plugins"', f'plugins_dir = "{folder}/plugins"')
    cfg = cfg.replace('# ui_dir = "/opt/godwinmix/ui"', f'ui_dir = "{REPO}/ui"')
    os.makedirs(f"{folder}/plugins")
    open(f"{folder}/godwinmix.toml", "w").write(cfg + f"\n[plugins.ingest]\nrtmp_port = {free_port()}\n")
    env = {**os.environ, "GODWINMIX_RUNTIME_DIR": f"{folder}/runtime", "GODWINMIX_HOME": f"{folder}/home"}
    env.pop("GODWINMIX_TOKEN", None)
    log = open(f"{folder}/log.jsonl", "w")
    return subprocess.Popen([os.path.join(BIN, "godwinmix"), "--config", f"{folder}/godwinmix.toml", "--log-format", "json"],
                            env=env, stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)


def stage_ingest(folder):
    src, to = os.path.join(REPO, "plugins", "ingest"), os.path.join(folder, "ingest-src")
    os.makedirs(os.path.join(to, "bin"))
    for f in ["gmx-plugin.toml", "README.md"]:
        shutil.copy(os.path.join(src, f), to)
    for d in ["schemas", "skills", "designer"]:
        shutil.copytree(os.path.join(src, d), os.path.join(to, d))
    shutil.copy(os.path.join(BIN, "gmx-ingest"), os.path.join(to, "bin"))
    return to


def wait_up(port):
    for _ in range(360):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/api/v1/core/status", timeout=2)
            return
        except Exception:
            time.sleep(0.5)
    raise SystemExit("the station did not answer within three minutes")


def shows(count, single_a, single_b):
    out = []
    for i in range(count):
        c = channel(i)
        g, p = divmod(i, PER_GROUP)
        if g < GROUPS:
            inp = {"uri": f"udp://@239.10.0.{g + 1}:{5001 + g}", "program": p + 1}
        else:
            inp = {"uri": f"udp://127.0.0.1:{single_a if i % 2 == 0 else single_b}"}
        out.append({"name": f"{c['n']} {c['name']}", "compositing": False, "input": inp, "outputs": [{"id": "hls", "uri": "hls://tv"}]})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=0, help="the station's port; a free one when left out")
    ap.add_argument("--shows", type=int, default=50)
    args = ap.parse_args()
    args.port = args.port or free_port()
    folder = tempfile.mkdtemp(prefix="gmx-headend-wall-")
    feeds, single_a, single_b = [], free_port(), free_port()
    chans = [channel(i) for i in range(args.shows)]
    for g in range(min(GROUPS - 1, args.shows // PER_GROUP)):
        feeds.append(gst(mux(chans[g * PER_GROUP:(g + 1) * PER_GROUP], f"239.10.0.{g + 1}", 5001 + g, drop=g == 2)))
    if args.shows > GROUPS * PER_GROUP:
        feeds.append(gst(mux(chans[GROUPS * PER_GROUP:GROUPS * PER_GROUP + 1], "127.0.0.1", single_a)))
    st = station(folder, args.port)

    done = []

    def stop(*_):
        if done:
            sys.exit(0)
        done.append(1)
        for p in feeds + [st]:
            try:
                os.killpg(p.pid, signal.SIGINT)
            except OSError:
                pass
        for p in feeds + [st]:
            try:
                p.wait(timeout=20)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except OSError:
                    pass
        shutil.rmtree(folder, ignore_errors=True)
        print("stopped the feeds and the station, removed", folder)
        sys.exit(0)

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    try:
        start(args, folder, single_a, single_b)
    except BaseException as e:
        print("stopping:", e, "; the station's log was", f"{folder}/log.jsonl")
        stop()


def start(args, folder, single_a, single_b):
    wait_up(args.port)
    print(post(args.port, "plugins", {"source": stage_ingest(folder)}).get("error") or "ingest plugin added")
    made = post(args.port, "shows/add_many", {"shows": shows(args.shows, single_a, single_b), "dry_run": False})
    print(made.get("error") or f"{len(made['result']['added'])} shows added")
    if args.shows >= 49:
        slug = made['result']['added'][48]
        print(post(args.port, f"shows/{slug}/stop", {}).get("error") or f"{slug} stopped")
    print(f"http://127.0.0.1:{args.port}/   folder {folder}   Ctrl+C stops everything", flush=True)
    while True:
        time.sleep(3600)


if __name__ == "__main__":
    main()
