#!/usr/bin/env python3
"""Fail when a Linux release artefact needs more than the baseline machine has.

Every ELF file under the paths given is read, including the files inside a
.deb and an AppImage, and three things are checked against the baseline:

  * the newest glibc symbol version it imports (GLIBC_2.35 for Ubuntu 22.04,
    Debian 12 and Raspberry Pi OS bookworm and later),
  * the newest libstdc++ symbol version (GLIBCXX_3.4.30, the same systems),
  * on x86_64, the "x86 ISA needed" note, which must say nothing newer than
    baseline x86-64. A v3 note is what made the 0.3.29 release refuse to
    start on a 2010 Xeon.

The 0.3.30 mixer imported GLIBC_2.43 and started only on Ubuntu 26.04. This
is the check that would have stopped it.

Usage:
    dev/check-linux-baseline.py [--glibc 2.35] [--glibcxx 3.4.30] PATH...

Needs readelf (binutils), and dpkg-deb for a .deb. Exit status 1 names every
file over the line.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ELF = b"\x7fELF"
X86_LEVELS = {"x86-64-baseline": 1, "x86-64-v2": 2, "x86-64-v3": 3, "x86-64-v4": 4}


def version(text: str) -> tuple[int, ...]:
    return tuple(int(p) for p in text.split("."))


def is_elf(path: Path) -> bool:
    try:
        with path.open("rb") as f:
            return f.read(4) == ELF
    except OSError:
        return False


def readelf(args: list[str], path: Path) -> str:
    out = subprocess.run(["readelf", "-W", *args, str(path)],
                         capture_output=True, text=True)
    return out.stdout


def newest(path: Path, prefix: str) -> str | None:
    """The newest PREFIX_x.y symbol version the file needs."""
    found = re.findall(rf"\b{prefix}_([0-9.]+)\b", readelf(["--version-info"], path))
    return max(found, key=version) if found else None


def isa_needed(path: Path) -> str | None:
    """The highest x86 ISA level the file's property note asks for."""
    for line in readelf(["-n"], path).splitlines():
        if "x86 ISA needed" in line:
            levels = [l.strip() for l in line.split(":", 1)[1].split(",")]
            known = [l for l in levels if l in X86_LEVELS]
            return max(known, key=X86_LEVELS.get) if known else None
    return None


def unpack(path: Path, into: Path) -> Path:
    """A .deb or an AppImage, unpacked so its files can be walked."""
    if path.suffix == ".deb":
        subprocess.run(["dpkg-deb", "-x", str(path), str(into)], check=True)
        return into
    # An AppImage is an ELF runtime with a squashfs after it, and extracts
    # itself without FUSE. Copied first so a read only mount is no obstacle.
    copy = into / path.name
    copy.write_bytes(path.read_bytes())
    copy.chmod(0o755)
    subprocess.run([str(copy), "--appimage-extract"], cwd=into, check=True,
                   stdout=subprocess.DEVNULL)
    return into / "squashfs-root"


def files(paths: list[Path], scratch: Path):
    for path in paths:
        if path.is_file() and (path.suffix == ".deb" or path.name.endswith(".AppImage")):
            root = unpack(path, Path(tempfile.mkdtemp(dir=scratch)))
            yield from ((path.name, f) for f in walk(root))
        elif path.is_dir():
            yield from ((str(path), f) for f in walk(path))
        elif is_elf(path):
            yield str(path), path


def walk(root: Path):
    for dirpath, _, names in os.walk(root):
        for name in names:
            f = Path(dirpath) / name
            if not f.is_symlink() and is_elf(f):
                yield f


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--glibc", default="2.35")
    ap.add_argument("--glibcxx", default="3.4.30")
    ap.add_argument("paths", nargs="+", type=Path)
    args = ap.parse_args()

    over = []
    count = 0
    with tempfile.TemporaryDirectory() as scratch:
        for source, f in files(args.paths, Path(scratch)):
            count += 1
            name = f"{source}: {f.name}"
            g = newest(f, "GLIBC")
            if g and version(g) > version(args.glibc):
                over.append(f"{name} needs GLIBC_{g}, over {args.glibc}")
            cxx = newest(f, "GLIBCXX")
            if cxx and version(cxx) > version(args.glibcxx):
                over.append(f"{name} needs GLIBCXX_{cxx}, over {args.glibcxx}")
            isa = isa_needed(f)
            if isa and X86_LEVELS[isa] > 1:
                over.append(f"{name} is marked {isa}, so a CPU without it refuses to load it")

    if count == 0:
        print("no ELF files found in what was given", file=sys.stderr)
        return 1
    for line in over:
        print(f"::error::{line}" if os.environ.get("GITHUB_ACTIONS") else line)
    print(f"{count} ELF files checked against glibc {args.glibc}, "
          f"libstdc++ {args.glibcxx} and baseline x86-64: "
          f"{'all within' if not over else f'{len(over)} over the line'}")
    return 1 if over else 0


if __name__ == "__main__":
    sys.exit(main())
