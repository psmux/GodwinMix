#!/usr/bin/env python3
"""Print every OSC message gmx-osc sends out.

    python3 dev/osc-listen.py 9001 [--for 20]

Binds the port and prints one line per message until the time runs out. Bind it
before the plugin starts sending: a datagram to a port with nothing on it earns
an ICMP unreachable, and the sender only finds out on its next send.

Standard library only. The decoder is the half of OSC needed to read what the
bridge writes: an address, a type tag string, and int, float and string
arguments, everything four byte aligned and big endian.
"""

import socket
import struct
import sys
import time


def take_str(packet, at):
    end = packet.index(b"\0", at)
    return packet[at:end].decode("utf-8", "replace"), (end + 4) & ~3


def decode(packet):
    address, at = take_str(packet, 0)
    args = []
    if at < len(packet):
        tags, at = take_str(packet, at)
        for tag in tags[1:]:
            if tag == "i":
                args.append(struct.unpack_from(">i", packet, at)[0])
                at += 4
            elif tag == "f":
                args.append(round(struct.unpack_from(">f", packet, at)[0], 4))
                at += 4
            elif tag == "s":
                value, at = take_str(packet, at)
                args.append(value)
            elif tag in "TFN":
                args.append({"T": True, "F": False, "N": None}[tag])
    return address, args


def main():
    port = int(sys.argv[1])
    seconds = 20.0
    if "--for" in sys.argv:
        seconds = float(sys.argv[sys.argv.index("--for") + 1])

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", port))
    sock.settimeout(0.5)

    deadline = time.monotonic() + seconds
    seen = 0
    while time.monotonic() < deadline:
        try:
            packet, _ = sock.recvfrom(4096)
        except socket.timeout:
            continue
        address, args = decode(packet)
        print(f"{address} {args}", flush=True)
        seen += 1
    return 0 if seen else 1


if __name__ == "__main__":
    sys.exit(main())
