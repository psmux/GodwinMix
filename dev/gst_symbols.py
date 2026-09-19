"""Remove debug sections from copied runtimes without removing exports."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
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
                              capture_output=True, text=True).stdout.strip()
        tools = sorted(Path(root).glob("lib/rustlib/*/bin/llvm-objcopy.exe"))
        if tools:
            return str(tools[0])
    for name in ("llvm-strip", "llvm-objcopy"):
        if tool := shutil.which(name):
            return tool
    return shutil.which("strip") if platform == "linux" else None


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
        before = path.stat().st_size
        result = subprocess.run([tool, "--strip-debug", str(path)],
                                capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(f"cannot strip debug sections from {path}: {result.stderr.strip()}")
        saved += before - path.stat().st_size
    print(f"removed {saved / (1024 * 1024):.1f} MB of debug sections")
