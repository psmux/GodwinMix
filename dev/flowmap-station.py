#!/usr/bin/env python3
"""A station with something of every kind on it, for working on the flow map.

Builds on dev/headend-wall.py: its eight multicast groups of six programs
(one group with no sender, one losing a packet in a thousand) and fifty
direct shows, each with an HLS output. On top of that:

  * two in three of those HLS outputs convert to 640x360, some serving TS
    beside CMAF and some open without a key;
  * channel 104 also sends on by UDP, 107 by RTMP to a server that is not
    there (so it keeps reconnecting), and 110 by SRT;
  * "Relay 1", a show whose input is RTMP, played from ffmpeg;
  * channel 150 is fed by its own sender, which is cut after --cut seconds,
    so its link goes dead while the map watches;
  * the station's mixed show gets two test pictures, a file and an output.

With --mixed-only there are no channels at all: one mixed show, its
sources and an output, which is what most stations are.

    python3 dev/flowmap-station.py [--port N] [--shows 50] [--cut 90] [--mixed-only]

Needs what headend-wall.py needs, and ffmpeg. Put dev/fake-nvidia-smi.py on
PATH as nvidia-smi for a GPU on the map. Ctrl+C stops every feed and the
station and removes the folder. Standard library only.
"""

import argparse, importlib.util, os, shutil, signal, subprocess, sys, tempfile, threading, time

spec = importlib.util.spec_from_file_location("wall", os.path.join(os.path.dirname(os.path.abspath(__file__)), "headend-wall.py"))
wall = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wall)


def converted(shows):
    for i, s in enumerate(shows):
        hls = s["outputs"][0]
        if i % 3:
            hls["rendition"] = {"video": {"codec": "h264", "width": 640, "height": 360, "bitrate_kbps": 800}, "audio": {"codec": "aac", "bitrate_kbps": 96}}
        hls["params"] = {"segments": "both" if i % 2 else "cmaf", "access": "open" if i % 4 == 0 else "keyed"}
    return shows


def extras(shows, udp_out, srt_out):
    by = {s["name"].split()[0]: s for s in shows}
    if "104" in by:
        by["104"]["outputs"].append({"id": "lan", "uri": f"udp://127.0.0.1:{udp_out}"})
    if "107" in by:
        by["107"]["outputs"].append({"id": "relay", "uri": "rtmp://127.0.0.1:9/live/relay"})
    if "110" in by:
        by["110"]["outputs"].append({"id": "srt", "uri": f"srt://127.0.0.1:{srt_out}"})


def rtmp_server(port, stop):
    """ffmpeg serves one player at a time; serve it again whenever it ends."""
    line = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-re", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25", "-f", "lavfi", "-i", "sine=frequency=660",
            "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency", "-g", "50", "-b:v", "900k", "-c:a", "aac", "-b:a", "96k",
            "-f", "flv", "-listen", "1", f"rtmp://127.0.0.1:{port}/live/relay1"]
    procs = []

    def loop():
        while not stop:
            p = subprocess.Popen(line, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            procs.append(p)
            p.wait()
            time.sleep(0.5)
    threading.Thread(target=loop, daemon=True).start()
    return procs


def mixer(port, folder):
    clip = os.path.join(folder, "opener.mp4")
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=25:duration=20", "-f", "lavfi", "-i", "sine=duration=20",
                    "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest", clip])
    q = "?show=main"
    for body in [{"id": "cam1", "uri": "test://smpte", "name": "Camera 1"}, {"id": "cam2", "uri": "test://ball", "name": "Camera 2"},
                 {"id": "opener", "uri": f"file://{clip}", "name": "Opener"}]:
        print(wall.post(port, f"sources{q}", body).get("error") or f"source {body['name']} added")
    print(wall.post(port, f"program/take{q}", {"source": "cam1"}).get("error") or "Camera 1 on air")
    print(wall.post(port, f"outputs{q}", {"id": "youtube", "uri": "rtmp://127.0.0.1:9/live2/stream-key"}).get("error") or "an RTMP output added")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=0)
    ap.add_argument("--shows", type=int, default=50)
    ap.add_argument("--cut", type=int, default=90, help="seconds before channel 150's feed is cut; 0 never")
    ap.add_argument("--mixed-only", action="store_true")
    args = ap.parse_args()
    if args.mixed_only:
        args.shows = 0
    args.port = args.port or wall.free_port()
    folder = tempfile.mkdtemp(prefix="gmx-flowmap-")
    single_a, single_b, rtmp, udp_out, srt_out = (wall.free_port() for _ in range(5))
    chans = [wall.channel(i) for i in range(args.shows)]
    feeds, stop = [], []
    for g in range(min(wall.GROUPS - 1, args.shows // wall.PER_GROUP)):
        feeds.append(wall.gst(wall.mux(chans[g * wall.PER_GROUP:(g + 1) * wall.PER_GROUP], f"239.10.0.{g + 1}", 5001 + g, drop=g == 2)))
    cut = wall.gst(wall.mux(chans[49:50], "127.0.0.1", single_b)) if args.shows >= 50 else None
    served = [] if args.mixed_only else rtmp_server(rtmp, stop)
    st = wall.station(folder, args.port)

    def end(*_):
        stop.append(1)
        for p in feeds + served + [cut, st]:
            if p is None:
                continue
            try:
                os.killpg(p.pid, signal.SIGINT)
                p.wait(timeout=20)
            except (OSError, subprocess.TimeoutExpired):
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except OSError:
                    pass
        shutil.rmtree(folder, ignore_errors=True)
        print("stopped the feeds and the station, removed", folder, flush=True)
        sys.exit(0)

    signal.signal(signal.SIGINT, end)
    signal.signal(signal.SIGTERM, end)
    wall.wait_up(args.port)
    print(wall.post(args.port, "plugins", {"source": wall.stage_ingest(folder)}).get("error") or "ingest plugin added")
    mixer(args.port, folder)
    if not args.mixed_only:
        shows = converted(wall.shows(args.shows, single_a, single_b))
        extras(shows, udp_out, srt_out)
        shows.append({"name": "Relay 1", "compositing": False, "input": {"uri": f"rtmp://127.0.0.1:{rtmp}/live/relay1"}, "outputs": [{"id": "hls", "uri": "hls://tv"}, {"id": "lan", "uri": f"udp://127.0.0.1:{udp_out + 1}"}]})
        made = wall.post(args.port, "shows/add_many", {"shows": shows, "dry_run": False})
        print(made.get("error") or f"{len(made['result']['added'])} shows added")
        if args.shows >= 49:
            print(wall.post(args.port, f"shows/{made['result']['added'][48]}/stop", {}).get("error") or "channel 149 stopped")
    print(f"http://127.0.0.1:{args.port}/   folder {folder}   Ctrl+C stops everything", flush=True)
    t0 = time.time()
    while True:
        time.sleep(1)
        if cut and args.cut and time.time() - t0 > args.cut:
            os.killpg(cut.pid, signal.SIGINT)
            print("cut channel 150's feed", flush=True)
            cut = None


if __name__ == "__main__":
    main()
