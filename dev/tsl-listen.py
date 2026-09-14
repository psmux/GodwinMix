#!/usr/bin/env python3
"""Read TSL UMD v5 packets on a UDP port and print what each lamp says.

    python3 dev/tsl-listen.py 8900 [--for 8]

This is what a tally interface would do with the packets gmx-tally sends, and
it is how the live test checks the lamp bits rather than trusting that a
datagram left the machine.

The format, all little endian:

    PBC u16   byte count of everything after itself
    VER u8    0 for v5.0
    FLAGS u8  bit 0 set means the labels are UTF-16LE
    SCREEN u16
    then, repeated: INDEX u16, CONTROL u16, LENGTH u16, TEXT

CONTROL packs four two bit fields: right tally at 0, text tally at 2, left
tally at 4, brightness at 6. 0 off, 1 red, 2 green, 3 amber.
"""

import socket
import struct
import sys
import time

COLOURS = {0: "off", 1: "red", 2: "green", 3: "amber"}


def read(packet):
    if len(packet) < 8:
        return []
    pbc = struct.unpack_from("<H", packet, 0)[0]
    body = packet[2 : 2 + pbc]
    if not body or body[0] != 0:
        return []
    unicode_labels = bool(body[1] & 0x01)
    screen = struct.unpack_from("<H", body, 2)[0]

    lamps, at = [], 4
    while at + 6 <= len(body):
        index, control = struct.unpack_from("<HH", body, at)
        at += 4
        if control & 0x8000:
            continue
        length = struct.unpack_from("<H", body, at)[0]
        at += 2
        raw = body[at : at + length]
        at += length
        label = raw.decode("utf-16-le" if unicode_labels else "ascii", "replace")
        lamps.append(
            "screen=%d index=%d right=%s text=%s left=%s brightness=%d label=%r"
            % (
                screen,
                index,
                COLOURS[control & 3],
                COLOURS[(control >> 2) & 3],
                COLOURS[(control >> 4) & 3],
                (control >> 6) & 3,
                label,
            )
        )
    return lamps


def main():
    port = int(sys.argv[1])
    seconds = 8.0
    if "--for" in sys.argv:
        seconds = float(sys.argv[sys.argv.index("--for") + 1])

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", port))
    sock.settimeout(0.5)

    # A hard deadline as well as a socket timeout: the sender refreshes every
    # few seconds, so a listener that only stops on silence never stops.
    deadline = time.monotonic() + seconds
    seen = []
    while time.monotonic() < deadline:
        try:
            packet, _ = sock.recvfrom(2048)
        except socket.timeout:
            continue
        for lamp in read(packet):
            if lamp not in seen:
                seen.append(lamp)
                print(lamp, flush=True)
        # Enough to prove both lamps and a colour change.
        if len(seen) >= 3:
            break
    return 0 if seen else 1


if __name__ == "__main__":
    sys.exit(main())
