#!/usr/bin/env python3
"""Make the starter transitions and effects in graphics/starters/ with ffmpeg alone.

Every picture here is drawn by an ffmpeg expression, so the set is ours to
ship under the repository's licence and anyone can make it again:

    python3 dev/make-starter-fx.py

Writes the media file of each item. The graphic.toml beside each is written by
hand and checked in; tests/fx_import.rs checks that what the importer measures on
these files agrees with it. Needs an ffmpeg with libvpx-vp9.

The clips are small on purpose (640x360, a second or two, a high CRF). A
light leak, a burn or bokeh is soft light, and scaled to the canvas it
looks the same as one made at 1080p.
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "graphics" / "starters"
W, H, FPS = 640, 360, 30


def run(args):
    print("ffmpeg", args[-1], file=sys.stderr)
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", *args], check=True)


def vp9(out, seconds, expr, alpha=False):
    """A clip from per channel geq expressions, as VP9 in WebM."""
    fmt = "rgba" if alpha else "rgb24"
    src = f"color=c=black:s={W}x{H}:r={FPS}:d={seconds},format={fmt},geq={expr}"
    pix = ["-pix_fmt", "yuva420p", "-auto-alt-ref", "0"] if alpha else ["-pix_fmt", "yuv420p"]
    out.parent.mkdir(parents=True, exist_ok=True)
    run(["-f", "lavfi", "-i", src, "-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "42",
         "-row-mt", "1", "-deadline", "good", *pix, str(out)])


def light_leak():
    # Two warm blobs drifting across, swelling to a near white wash at one
    # second, which is where a take cuts under it.
    e = "pow(sin(PI*T/2)\\,2)"
    blob = lambda cx, cy, s: f"exp(-(pow(X-({cx})\\,2)+pow(Y-({cy})\\,2))/({s}))"
    a = blob("W*(0.1+0.45*T)", "H*0.35", "2*pow(W*0.30\\,2)")
    b = blob("W*(0.95-0.3*T)", "H*0.7", "2*pow(W*0.26\\,2)")
    wash = f"(1.25*{e}*({a}+0.8*{b})+0.9*pow({e}\\,4))"
    expr = (f"r='255*clip({wash}*1.15\\,0\\,1)':"
            f"g='255*clip({wash}*0.82\\,0\\,1)':"
            f"b='255*clip({wash}*0.55-0.1+0.6*pow({e}\\,6)\\,0\\,1)'")
    vp9(ROOT / "light-leak" / "light-leak.webm", 2, expr)


def bokeh():
    # Soft discs of light drifting up and across: an effect, never a cover.
    discs = []
    for i in range(9):
        cx = f"W*mod({0.11 * i + 0.05}+0.06*T\\,1)"
        cy = f"H*(1.1-mod({0.37 * i}+0.22*T\\,1.2))"
        r = 22 + (i * 7) % 30
        discs.append(f"clip(({r}-hypot(X-{cx}\\,Y-{cy}))/6\\,0\\,1)*{0.5 + (i % 3) * 0.2}")
    light = "(" + "+".join(discs) + ")*sin(PI*T/3)"
    expr = (f"r='255*clip({light}*1.0\\,0\\,1)':"
            f"g='255*clip({light}*0.8\\,0\\,1)':"
            f"b='255*clip({light}*0.55\\,0\\,1)'")
    vp9(ROOT / "bokeh" / "bokeh.webm", 3, expr)


def glitch():
    # Coloured blocks with alpha that fill the frame half way through and
    # clear again: a stinger, and a short hit when fired as an effect.
    cell = "(floor(X/40)*7+floor(Y/24)*13+floor(T*15)*17)"
    hash = f"mod({cell}*{cell}*31\\,97)/97"
    cover = "(1-abs(2*T-1))*1.35"
    alpha = f"255*lt({hash}\\,{cover})"
    expr = (f"r='255*mod({cell}*5\\,3)/2':g='40+200*mod({cell}*3\\,2)':"
            f"b='255*lt(mod({cell}\\,4)\\,2)':a='{alpha}'")
    vp9(ROOT / "glitch" / "glitch.webm", 1, expr, alpha=True)


def film_burn():
    # A burn eating in from the left to white at the middle and back out:
    # meant for Add.
    front = "(1.7*(1-abs(T/0.75-1))-0.35)"
    ragged = "0.08*sin(Y/17+T*9)+0.05*sin(Y/5.3-T*4)"
    v = f"clip(({front}-X/W+{ragged})*4\\,0\\,1)"
    expr = (f"r='255*clip({v}*1.6\\,0\\,1)':"
            f"g='255*clip({v}*1.2-0.15\\,0\\,1)':"
            f"b='255*clip({v}*1.6-0.6\\,0\\,1)'")
    vp9(ROOT / "film-burn" / "film-burn.webm", 1.5, expr)


def iris():
    # A circle opening from the middle: dark in the centre, light at the
    # corners, so the new scene shows from the centre out.
    out = ROOT / "iris" / "iris.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    expr = "lum='255*clip(hypot(X-W/2\\,Y-H/2)/hypot(W/2\\,H/2)\\,0\\,1)':cb=128:cr=128"
    run(["-f", "lavfi", "-i", f"color=c=black:s={W}x{H},format=yuv444p,geq={expr}",
         "-frames:v", "1", "-pix_fmt", "gray", str(out)])


def loops():
    """Each starter's preview strip as a short WebM loop, which the gallery
    plays while a pointer rests on its card. Run after the strips are drawn:
    cargo test -p godwinmix-core --test fx_import -- --ignored write_starter_previews
    """
    for strip in sorted(ROOT.glob("*/preview-strip.jpg")):
        run(["-i", str(strip), "-vf", "untile=12x1,setpts=N/(8*TB),format=yuv420p", "-r", "8",
             "-c:v", "libvpx", "-b:v", "0", "-crf", "30", "-an", str(strip.parent / "preview.webm")])


if __name__ == "__main__":
    only = sys.argv[1:]
    for make in (light_leak, bokeh, glitch, film_burn, iris, loops):
        if not only or make.__name__ in only:
            make()
    for f in sorted(ROOT.rglob("*.*")):
        print(f"{f.stat().st_size:>8}  {f.relative_to(ROOT)}")
