#!/usr/bin/env python3
"""Send an MPEG-TS file over UDP at its own bitrate, losing some on purpose.

    tests/lossy_send.py clip.ts 239.1.1.1 19471 --loss 1 --seconds 30
    tests/lossy_send.py clip.ts 239.1.1.1 19471 --rtp --loss 1

Seven packets to a datagram, paced by the PCR-free rule of thumb that the
file's size over its duration is its rate (so give it a constant bitrate
file; ffmpeg -muxrate makes one). `--loss` is a percentage of datagrams
dropped at random before they reach the socket, which is what a congested
switch port does. `--pause` stops sending for that many seconds half way,
for the feed that goes away and comes back. Standard library only.
"""

import argparse
import random
import socket
import struct
import subprocess
import time


def duration(path):
    out = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path],
                         capture_output=True, text=True).stdout.strip()
    return float(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("host")
    ap.add_argument("port", type=int)
    ap.add_argument("--loss", type=float, default=0.0)
    ap.add_argument("--seconds", type=float, default=30.0)
    ap.add_argument("--rtp", action="store_true")
    ap.add_argument("--pause", type=float, default=0.0)
    a = ap.parse_args()

    data = open(a.file, "rb").read()
    rate = len(data) / duration(a.file)  # bytes a second
    chunk = 7 * 188
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 1)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 1)
    sent = dropped = seq = 0
    t0 = time.time()
    offset = 0
    paused = False
    while time.time() - t0 < a.seconds:
        if a.pause and not paused and time.time() - t0 > a.seconds / 2:
            time.sleep(a.pause)
            paused = True
            t0 += a.pause
        piece = data[offset:offset + chunk]
        offset = offset + chunk if offset + chunk < len(data) else 0
        seq = (seq + 1) & 0xFFFF
        if random.random() * 100 < a.loss:
            dropped += 1
        else:
            if a.rtp:
                piece = struct.pack("!BBHII", 0x80, 33, seq, int(time.time() * 90000) & 0xFFFFFFFF, 0x6D6978) + piece
            sock.sendto(piece, (a.host, a.port))
            sent += 1
        # Pace to the file's rate: where the stream should be by now.
        ahead = (sent + dropped) * chunk / rate - (time.time() - t0)
        if ahead > 0:
            time.sleep(ahead)
    print(f"sent {sent} datagrams, dropped {dropped} on purpose ({100 * dropped / max(sent + dropped, 1):.2f}%)")


if __name__ == "__main__":
    main()
