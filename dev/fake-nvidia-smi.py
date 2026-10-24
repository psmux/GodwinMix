#!/usr/bin/env python3
"""A stand in for nvidia-smi, for working on the headend wall's GPU strip
on a machine with no NVIDIA card. It answers the one query the station
sends (crates/godwinmix/src/machine/smi.rs) with numbers that move like an
RTX 3060 carrying fifty channels, and refuses anything else.

    mkdir -p /tmp/fake && cp dev/fake-nvidia-smi.py /tmp/fake/nvidia-smi
    PATH=/tmp/fake:$PATH python3 dev/headend-wall.py

The station then reports `source: nvidia-smi` and the strip fills in. Never
put it on a machine with a real card.
"""

import math, sys, time

if not any(a.startswith("--query-gpu=") for a in sys.argv[1:]):
    sys.exit("this is a stand in for nvidia-smi; it answers --query-gpu only")
t = time.time()
wave = lambda period, lo, hi: lo + (hi - lo) * (0.5 + 0.5 * math.sin(t * 2 * math.pi / period))
enc, dec, gfx = wave(37, 84, 96), wave(23, 38, 52), wave(51, 22, 41)
used = int(wave(90, 5900, 6600))
temp, power = int(wave(120, 61, 69)), wave(29, 92, 124)
print(f"0, NVIDIA GeForce RTX 3060, 550.144.03, {gfx:.0f}, {enc:.0f}, {dec:.0f}, {used}, 12288, 51, 30, {temp}, {power:.2f}, 170.00, 1807, 7300, 1612, {int(wave(60, 44, 58))}")
