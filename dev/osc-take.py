#!/usr/bin/env python3
"""Send one OSC take to gmx-osc and check the mixer took it.

Ten lines of it are the sender. The rest is waiting for the answer, because a
test that sends a datagram and asserts nothing has tested nothing.

    python3 dev/osc-take.py http://127.0.0.1:8080 TOKEN 127.0.0.1:9000 cam2

Standard library only: OSC is four byte aligned strings and big endian numbers,
and that is the whole encoder.
"""

import json
import socket
import struct
import sys
import time
import urllib.request


def osc(address, *args):
    """One OSC message: address, type tags, arguments. All of OSC we need."""
    def pad(text):
        raw = text.encode() + b"\0"
        return raw + b"\0" * (-len(raw) % 4)

    tags = "," + "".join("s" if isinstance(a, str) else "f" for a in args)
    body = b"".join(pad(a) if isinstance(a, str) else struct.pack(">f", a) for a in args)
    return pad(address) + pad(tags) + body


def program(base, token):
    request = urllib.request.Request(f"{base}/api/v1/program", headers={"Authorization": f"Bearer {token}"})
    with urllib.request.urlopen(request, timeout=5) as answer:
        return json.load(answer).get("program")


def main():
    base, token, target, source = sys.argv[1:5]
    host, port = target.rsplit(":", 1)

    before = program(base, token)
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.sendto(osc("/program/take", source), (host, int(port)))

    # The take is a round trip through the bridge and the core, so give it a
    # moment rather than asserting on the next line.
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        now = program(base, token)
        if now == source:
            print(f"took {source}")
            return 0
        time.sleep(0.2)

    print(f"the programme is still {before!r}, not {source!r}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
