#!/usr/bin/env python3
"""Check what a template wrote: a Matroska header, a cluster, and a clean exit.

Used by test-templates.sh. It reads the media file and the control log a
template produced when fed a handshake, and reports every problem at once.

    check-first-cluster.py <media.mkv> <control.jsonl> <width> <height>
"""
import json
import sys


def main():
    media_path, control_path, width, height = sys.argv[1:5]
    width, height = int(width), int(height)
    media = open(media_path, "rb").read()
    control = []
    for raw in open(control_path, encoding="utf-8", errors="replace"):
        raw = raw.strip()
        if not raw:
            continue
        try:
            control.append(json.loads(raw))
        except ValueError:
            pass          # a non JSON line on stderr is a log line, not an error
    problems = []

    def want(condition, message):
        if not condition:
            problems.append(message)

    want(control and control[0].get("method") == "initialize",
         "the plugin must speak first, with initialize")
    if control:
        params = control[0].get("params") or {}
        want(params.get("api") == 1, "initialize must name api 1")
        want("container" in (params.get("transports") or []),
             "a template must declare the container transport")
        want(params.get("provides"), "initialize must carry the manifest's provides")
    want(any(m.get("method") == "initialized" for m in control),
         "the plugin must send initialized once the core has answered")
    answers = {m["id"]: m for m in control if "id" in m and "method" not in m}
    want(1 in answers and "result" in answers[1], "start was not answered")
    want(2 in answers and "result" in answers[2], "shutdown was not answered")

    want(media[:4] == b"\x1a\x45\xdf\xa3", "the stream must start with an EBML header")
    want(b"matroska" in media[:64], "the DocType must be matroska")
    want(b"V_UNCOMPRESSED" in media, "raw video is V_UNCOMPRESSED")
    want(b"I420" in media, "the ColourSpace fourcc must be I420")
    cluster = media.find(b"\x1f\x43\xb6\x75")
    want(cluster > 0, "no cluster was written within the second")
    frame_bytes = width * height * 3 // 2
    if cluster > 0:
        want(len(media) - cluster > frame_bytes,
             "the first cluster carries no whole %dx%d frame" % (width, height))
        block = media.find(b"\xa3", cluster)
        luma = media[block + 8:block + 8 + width]
        want(len(set(luma)) > 1, "the first luma row is one flat value, not a picture")

    if problems:
        print("FAIL")
        for p in problems:
            print("    " + p)
        return 1
    frames = (len(media) - cluster) // (frame_bytes + 8)
    print("    %d bytes of media, about %d frames, %d control lines"
          % (len(media), frames, len(control)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
