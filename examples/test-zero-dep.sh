#!/bin/sh
# Drive examples/zero-dep-source.py with a hand written handshake and read the
# first cluster back. No core, no GStreamer, no Python packages: python3 and a
# shell are the whole list.
#
# What it proves: the plugin speaks first, accepts the core's answer, writes a
# Matroska header and at least one cluster of colour bars at the canvas caps,
# answers health while producing, and exits on shutdown.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
plugin="$here/zero-dep-source.py"
python=${GMX_PYTHON:-python3}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

canvas_w=160
canvas_h=90
canvas_fps=30

cat > "$work/core.jsonl" <<EOF
{"jsonrpc":"2.0","id":0,"result":{"core":"godwinmix","version":"0.0.0","api_level":1,"api_compatible":1,"canvas":{"width":$canvas_w,"height":$canvas_h,"fps":$canvas_fps},"transport":"container","media":"","instance":"bars","provide":"source","params":{}}}
{"jsonrpc":"2.0","id":1,"method":"start","params":{"canvas":{"width":$canvas_w,"height":$canvas_h,"fps":$canvas_fps},"transport":"container","media":""}}
{"jsonrpc":"2.0","id":2,"method":"health","params":{}}
EOF

# The core's lines, then a pause while frames are produced, then shutdown.
{
    cat "$work/core.jsonl"
    sleep 1
    printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{"reason":"test"}}'
    sleep 1
} | "$python" "$plugin" > "$work/media.mkv" 2> "$work/control.jsonl" || true

"$python" - "$work/media.mkv" "$work/control.jsonl" "$canvas_w" "$canvas_h" "$canvas_fps" <<'PY'
import json, sys

media_path, control_path, width, height, fps = sys.argv[1:6]
width, height, fps = int(width), int(height), int(fps)
media = open(media_path, "rb").read()
control = [json.loads(l) for l in open(control_path) if l.strip()]
problems = []


def want(condition, message):
    if not condition:
        problems.append(message)


# The control channel.
want(control and control[0].get("method") == "initialize",
     "the plugin must speak first, with initialize")
want(control[0]["params"]["api"] == 1, "initialize must name api 1")
want(control[0]["params"]["transports"] == ["container"],
     "this plugin declares only the container transport")
want(any(m.get("method") == "initialized" for m in control),
     "the plugin must send initialized after the core's answer")
answers = {m.get("id"): m for m in control if "id" in m and m.get("method") is None}
want(1 in answers and "result" in answers[1], "start was not answered")
want(2 in answers and answers[2]["result"]["state"] == "ok",
     "health must be answered, and answered while frames are going out")
want(3 in answers and "result" in answers[3], "shutdown was not answered")
for message in control:
    want("jsonrpc" in message, "every line is a JSON-RPC object: %r" % message)

# The media stream.
want(media[:4] == b"\x1a\x45\xdf\xa3", "the stream must start with an EBML header")
want(b"matroska" in media[:64], "the DocType must be matroska")
want(b"V_UNCOMPRESSED" in media, "raw video is V_UNCOMPRESSED")
want(b"I420" in media, "the ColourSpace fourcc must be I420")
cluster = media.find(b"\x1f\x43\xb6\x75")
want(cluster > 0, "no cluster was written")
frame_bytes = width * height * 3 // 2
want(len(media) - cluster > frame_bytes, "the first cluster carries no whole frame")
frames = (len(media) - cluster) // (frame_bytes + 8)
want(frames >= fps // 3, "expected at least %d frames in a second, counted about %d"
     % (fps // 3, frames))

# The bars themselves: the first luma row must start white and end black.
first = media.find(b"\xa3", cluster)
luma = media[first + 8:first + 8 + width]
want(luma[:4] == b"\xeb" * 4, "the first bar is white (Y=235)")
want(luma[-4:] == b"\x10" * 4, "the last bar is black (Y=16)")

if problems:
    print("FAIL")
    for p in problems:
        print("  " + p)
    sys.exit(1)
print("ok: %d control lines, %d bytes of media, about %d frames of %dx%d bars"
      % (len(control), len(media), frames, width, height))
PY
