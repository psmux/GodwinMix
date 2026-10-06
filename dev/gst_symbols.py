"""Remove debug sections from copied runtimes without removing exports."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import struct
import subprocess


def strip_tool(platform: str) -> str | None:
    explicit = os.environ.get("GST_STRIP")
    if explicit:
        return explicit
    if platform == "windows":
        # MinGW DLLs can contain GNU COFF auxiliary symbols that LLVM rejects
        # as invalid SymbolTableIndex. GNU strip understands its own format.
        for directory in ("C:/mingw64/bin", "C:/msys64/mingw64/bin",
                          "C:/msys64/ucrt64/bin", "C:/msys64/usr/bin"):
            candidate = Path(directory) / "strip.exe"
            if candidate.is_file():
                return str(candidate)
    if platform == "windows" and shutil.which("rustc"):
        root = subprocess.run(["rustc", "--print", "sysroot"], check=True,
                              capture_output=True, text=True, encoding="utf-8", errors="replace").stdout.strip()
        tools = sorted(Path(root).glob("lib/rustlib/*/bin/llvm-objcopy.exe"))
        if tools:
            return str(tools[0])
    for name in ("llvm-strip", "llvm-objcopy"):
        if tool := shutil.which(name):
            return tool
    return shutil.which("strip") if platform == "linux" else None


def pe_has_debug(data: bytes) -> bool:
    """Whether a PE file carries anything --strip-debug would remove.

    An MSVC build keeps its debug information in a .pdb beside it, so its DLLs
    have no COFF symbol table and no .debug sections, and there is nothing to
    strip. Only a MinGW build carries them. GNU strip rewrites every file it is
    given even when it removes nothing, and on the official 1.28.6 MSVC
    runtime that rewrite broke OpenSSL (`LoadLibrary` failed with error 998,
    invalid access to memory location) and with it fourteen plugins: srt,
    dtls, webrtc, nice, soup, curl, rtmp, png, gdkpixbuf, rsvg, pango, opengl,
    svtav1 and x265. GitHub's Windows image has GNU strip in C:\\mingw64 and a
    developer's machine usually does not, which is why only the runner broke.
    """
    if data[:2] != b"MZ" or len(data) < 0x40:
        return False
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        return False
    sections, = struct.unpack_from("<H", data, pe + 6)
    symbols_at, symbols = struct.unpack_from("<II", data, pe + 12)
    if symbols_at or symbols:
        return True
    optional, = struct.unpack_from("<H", data, pe + 20)
    base = pe + 24 + optional
    for i in range(sections):
        name = data[base + i * 40:base + i * 40 + 8]
        # A long section name such as .debug_info is written as "/4", an
        # offset into the symbol table, which this file has none of anyway.
        if name.startswith((b".debug", b"/")):
            return True
    return False


def strip_debug(root: Path, platform: str) -> None:
    tool = strip_tool(platform)
    if tool is None:
        raise RuntimeError("no debug stripping tool; install llvm-tools with rustup "
                           "on Windows or binutils on Linux, or set GST_STRIP")
    saved = 0
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        with path.open("rb") as stream:
            magic = stream.read(4)
        if not (magic.startswith(b"MZ") or magic == b"\x7fELF"):
            continue
        if magic.startswith(b"MZ") and not pe_has_debug(path.read_bytes()):
            continue
        before = path.stat().st_size
        result = subprocess.run([tool, "--strip-debug", str(path)],
                                capture_output=True, text=True, encoding="utf-8", errors="replace")
        if result.returncode:
            raise RuntimeError(f"cannot strip debug sections from {path}: {result.stderr.strip()}")
        saved += before - path.stat().st_size
    print(f"removed {saved / (1024 * 1024):.1f} MB of debug sections")
