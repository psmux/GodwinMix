#!/usr/bin/env python3
"""Trim a GStreamer runtime down to what GodwinMix actually loads.

The stock GStreamer runtime is far too big to put inside a desktop installer.
The Windows one is 527 MB as a separate download and the budget for the whole
GodwinMix installer is 150 MB. This script is the shared part of the three
platform wrappers (`dev/bundle-gstreamer.sh` and `dev/bundle-gstreamer.ps1`):
given a GStreamer prefix that is already unpacked somewhere, it works out
which plugins are needed, copies those and the libraries they actually link
against, and leaves everything else behind.

Nothing here is a hand written list of file names. The plugins to keep are
worked out from three sources:

  * `codecs.toml`, the codec catalogue. Every element an entry can select is
    looked up in the registry and the plugin it lives in is kept. A codec
    added to the catalogue travels automatically.
  * `PIPELINE`, below: the elements the core builds by name rather than
    through the catalogue (the compositor's neighbours, the muxers, the two
    network sinks, the preview path). They are listed here because they are
    string literals in the Rust, and the list says where each came from.
  * `PLATFORM`, below: the capture, audio and hardware plugins that only exist
    on one operating system.

The libraries are not a list at all. Every kept plugin and every kept binary
is read for its dynamic imports and the closure of that is what gets copied,
so a plugin that quietly grew a dependency brings it along and a library
nothing references is dropped.

Usage:

    dev/gst_trim.py --from /opt/homebrew/opt/gstreamer \\
                    --out tauri-app/gstreamer/macos \\
                    --platform macos --budget-mb 130

Exit status is 0 when the tree was built and fits the budget, 1 otherwise.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

# --------------------------------------------------------------- the wanted --

# Elements the core makes by name. Collected from the Rust with:
#   grep -rhoE '(make|find)\("[a-z0-9_]+"\)' --include='*.rs' crates plugins
# plus the two lists in crates/godwinmix-core/src/observe/doctor.rs. Keep this
# in step when a pipeline gains an element; the platforms.yml job asserts the
# important ones are loadable out of the trimmed tree.
PIPELINE = [
    # the canvas and the mix
    "compositor", "audiomixer", "videoconvert", "videoscale", "videorate",
    "audioconvert", "audioresample", "audiorate", "volume", "level", "alpha",
    "capsfilter", "queue", "tee", "identity", "fakesink", "input-selector",
    # what a source is made of
    "decodebin", "uridecodebin", "filesrc", "filesink", "fdsrc",
    "videotestsrc", "audiotestsrc", "timeoverlay", "textoverlay",
    "souphttpsrc", "rtspsrc", "udpsrc", "typefind",
    # containers, in and out
    "flvmux", "flvdemux", "mp4mux", "matroskamux", "mpegtsmux", "oggmux",
    "webmmux", "qtdemux", "matroskademux", "tsdemux", "oggdemux",
    "h264parse", "h265parse", "aacparse", "av1parse", "opusparse",
    # out to the world
    "rtmp2sink", "rtmp2src", "srtsink", "srtsrc", "livesync",
    "rtph264pay", "rtpopuspay", "rtpjitterbuffer", "whipsink", "whepsrc",
    "webrtcbin", "dtlssrtpenc",
    # the multiview mosaic, the snapshots and the meters
    "jpegenc", "jpegdec", "multipartmux", "videobox", "videocrop",
    # the compositor slots (a flip per slot, a valve on the programme
    # return), the meters' opus and the node bridge's rtp. `videoflip` was
    # the one the first bundled app on a macOS runner could not find.
    "videoflip", "valve", "opusenc", "rtpbin", "udpsink",
    # the sidecar media contract: a container on a pipe everywhere, unixfd
    # where the platform has it
    "proxysink", "proxysrc", "unixfdsink", "unixfdsrc", "appsrc", "appsink",
    # what the operator hears and sees locally
    "autovideosink", "autoaudiosink", "autoaudiosrc", "autovideosrc",
    "glimagesink", "wpesrc",
]

# One operating system each. A name that is not in the registry is skipped in
# silence, which is what makes this list safe to share between platforms.
PLATFORM = {
    "windows": [
        # capture and playback, with the fallbacks the PRD's risk table names
        # for the two open wasapi2 bugs and the mfvideosrc startup bug
        "wasapi2sink", "wasapi2src", "wasapisink", "wasapisrc",
        "directsoundsink", "mfvideosrc", "ksvideosrc", "d3d11screencapturesrc",
        # hardware encode and decode, which is the reason to be on Windows
        "mfh264enc", "d3d11h264dec", "d3d11convert", "d3d11compositor",
        "d3d11videosink", "d3d12h264dec", "d3d12convert", "d3d12compositor",
        "qsvh264enc", "qsvh264dec", "nvh264enc", "nvh264dec", "amfh264enc",
    ],
    "macos": [
        "osxaudiosink", "osxaudiosrc", "avfvideosrc", "osxvideosink",
        "vtenc_h264", "vtenc_h264_hw", "vtenc_h265_hw", "vtdec", "vtdec_hw",
        "glimagesink",
    ],
    "linux": [
        "pulsesink", "pulsesrc", "alsasink", "alsasrc", "pipewiresrc",
        "v4l2src", "v4l2h264enc", "v4l2h264dec",
        "vah264enc", "vah264dec", "vacompositor", "vapostproc",
        "nvh264enc", "nvh264dec", "ximagesrc",
    ],
}

# Plugins with no element the catalogue or the pipelines name, which still
# have to travel. GStreamer will not start without the first two.
ALWAYS_PLUGINS = ["coreelements", "typefindfunctions", "playback", "app"]

# Whole directories that never belong in a bundle, matched on any path part.
NEVER = [
    "include", "pkgconfig", "gtk-doc", "man", "doc", "docs", "gir-1.0",
    "girepository-1.0", "python", "python3", "perl5", "cmake", "aclocal",
    "gst-validate", "gstreamer-1.0/validate", "ges", "systemd", "dbus-1",
]
# And file suffixes: development files and debug information.
NEVER_SUFFIX = [".a", ".la", ".lib", ".pc", ".h", ".hpp", ".pdb", ".exp",
                ".def", ".gir", ".typelib", ".pyc"]

# The binaries worth carrying. gst-inspect is what --headless-check and
# `gmx doctor` ask to prove the bundled registry is the loaded one.
KEEP_BINARIES = ["gst-inspect-1.0", "gst-launch-1.0", "gst-device-monitor-1.0"]


# ------------------------------------------------------------- the registry --


def catalogue_elements(codecs: Path) -> set[str]:
    """Every element name `codecs.toml` can select.

    Parsed rather than imported: this script runs before anything is built and
    on a machine that may not have a TOML reader for the Python it has.
    """
    wanted: set[str] = set()
    if not codecs.is_file():
        return wanted
    single = ("encoder", "decoder", "download", "upload", "parser", "convert",
              "compositor", "muxer")
    for line in codecs.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        m = re.match(r"^(\w+)\s*=\s*\"([A-Za-z0-9_]+)\"$", line)
        if m and m.group(1) in single:
            wanted.add(m.group(2))
            continue
        m = re.match(r"^requires\s*=\s*\[(.*)\]$", line)
        if m:
            wanted.update(re.findall(r'"([^"]+)"', m.group(1)))
    return wanted


def inspect(prefix: Path, plugins: Path, registry: Path, args: list[str]) -> str:
    """Run the source tree's own `gst-inspect-1.0` against the source tree.

    The registry goes to a scratch file so this never touches the registry of
    a GStreamer installed on the machine, and `GST_PLUGIN_SYSTEM_PATH` is
    pinned so a system install cannot answer for a plugin that is not in the
    tree being trimmed.
    """
    exe = which_in(prefix, "gst-inspect-1.0")
    if exe is None:
        die(f"no gst-inspect-1.0 under {prefix}; is that a GStreamer prefix?")
    env = dict(os.environ)
    env["GST_PLUGIN_PATH"] = str(plugins)
    env["GST_PLUGIN_SYSTEM_PATH"] = str(plugins)
    env["GST_REGISTRY"] = str(registry)
    scanner = which_in(prefix, "gst-plugin-scanner", SCANNER_DIRS)
    if scanner:
        env["GST_PLUGIN_SCANNER"] = str(scanner)
    libdir = prefix / "lib"
    if libdir.is_dir():
        for var in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
            env[var] = str(libdir) + os.pathsep + env.get(var, "")
    out = subprocess.run([str(exe), *args], env=env, capture_output=True,
                         text=True)
    return out.stdout


def element_map(prefix: Path, plugins: Path, registry: Path) -> dict[str, str]:
    """element name to plugin name, from one `gst-inspect-1.0` with no args.

    Its output is `plugin: element: description`, one feature per line, which
    is the whole map in a single process.
    """
    found: dict[str, str] = {}
    for line in inspect(prefix, plugins, registry, []).splitlines():
        m = re.match(r"^\s*([a-z0-9_-]+):\s+([A-Za-z0-9_-]+):\s", line)
        if m:
            found.setdefault(m.group(2), m.group(1))
    if not found:
        die("gst-inspect-1.0 listed no plugins; the source prefix looks wrong")
    return found


def plugin_file(prefix: Path, plugins: Path, registry: Path,
                name: str, licence: dict[str, str] | None = None) -> Path | None:
    """Where a plugin lives, and under what licence, asked of GStreamer
    rather than guessed."""
    found = None
    # Read the whole answer before returning. `Filename` comes before
    # `License` in what gst-inspect prints, so returning at the first match
    # would mean never learning the licence, which is how a GPL plugin gets
    # into a build nobody meant to make copyleft.
    for line in inspect(prefix, plugins, registry, [name]).splitlines():
        m = re.match(r"^\s*License\s+(.+?)\s*$", line)
        if m and licence is not None:
            licence[name] = m.group(1)
            continue
        m = re.match(r"^\s*Filename:?\s+(.+?)\s*$", line)
        if m and found is None:
            path = Path(m.group(1))
            if path.is_file():
                found = path
    if found is not None:
        return found
    # Every platform names the file the same way, so this is only the fallback
    # for a gst-inspect that answered oddly.
    for suffix in (".dylib", ".so", ".dll"):
        guess = plugins / f"libgst{name}{suffix}"
        if guess.is_file():
            return guess
    return None


# ---------------------------------------------------------- dynamic imports --


def imports_macho(path: Path) -> list[str]:
    out = subprocess.run(["otool", "-L", str(path)], capture_output=True,
                         text=True).stdout
    names = []
    for line in out.splitlines()[1:]:
        line = line.strip()
        if line.startswith("/") or line.startswith("@"):
            names.append(line.split(" ")[0])
    return names


def imports_elf(path: Path) -> list[str]:
    out = subprocess.run(["objdump", "-p", str(path)], capture_output=True,
                         text=True).stdout
    if not out:
        out = subprocess.run(["readelf", "-d", str(path)], capture_output=True,
                             text=True).stdout
    return re.findall(r"NEEDED\s+\[?([^\s\]]+)", out)


def imports_pe(path: Path) -> list[str]:
    """The DLL names in a PE file's import table, read here rather than with a
    tool, because a Windows runner has no dumpbin without Visual Studio and a
    GitHub image should not have to install one to measure an installer.

    The walk is the documented one: the DOS stub points at the PE header, the
    optional header's second data directory is the import table, and the
    section headers turn its address into a file offset.
    """
    data = path.read_bytes()
    if data[:2] != b"MZ":
        return []
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        return []
    sections, = struct.unpack_from("<H", data, pe + 6)
    opt_size, = struct.unpack_from("<H", data, pe + 20)
    opt = pe + 24
    magic, = struct.unpack_from("<H", data, opt)
    # The one field that moves between PE32 and PE32+ is BaseOfData, so the
    # data directory starts 16 bytes later in a 64 bit image.
    dirs = opt + (112 if magic == 0x20B else 96)
    count, = struct.unpack_from("<I", data, opt + (108 if magic == 0x20B else 92))
    if count < 2:
        return []
    import_rva, import_size = struct.unpack_from("<II", data, dirs + 8)
    if not import_rva:
        return []

    table = []
    base = pe + 24 + opt_size
    for i in range(sections):
        head = base + i * 40
        virt_size, virt_addr, raw_size, raw_ptr = struct.unpack_from("<IIII", data, head + 8)
        table.append((virt_addr, max(virt_size, raw_size), raw_ptr))

    def offset(rva: int) -> int | None:
        for virt_addr, size, raw_ptr in table:
            if virt_addr <= rva < virt_addr + size:
                return raw_ptr + (rva - virt_addr)
        return None

    names = []
    start = offset(import_rva)
    if start is None:
        return []
    # An array of 20 byte descriptors, ended by an all zero one.
    for i in range(0, import_size or 20 * 4096, 20):
        entry = start + i
        if entry + 20 > len(data):
            break
        fields = struct.unpack_from("<IIIII", data, entry)
        if not any(fields):
            break
        name_at = offset(fields[3])
        if name_at is None:
            break
        end = data.index(b"\0", name_at)
        names.append(data[name_at:end].decode("ascii", "replace"))
    return names


def imports(path: Path, platform: str) -> list[str]:
    try:
        if platform == "macos":
            return imports_macho(path)
        if platform == "windows":
            return imports_pe(path)
        return imports_elf(path)
    except Exception as e:                        # noqa: BLE001
        warn(f"could not read the imports of {path.name}: {e}")
        return []


def is_system(name: str, platform: str) -> bool:
    """A library the operating system provides, which must never be copied.

    Copying one is worse than missing it: a bundled libSystem or a bundled
    kernel32 either refuses to load or takes the process down.
    """
    leaf = name.rsplit("/", 1)[-1].rsplit("\\", 1)[-1]
    low = leaf.lower()
    if platform == "macos":
        return name.startswith(("/usr/lib/", "/System/"))
    if platform == "windows":
        return (low.startswith(("api-ms-win-", "ext-ms-", "vcruntime", "msvcp",
                                "ucrtbase", "concrt"))
                or low in {
                    "kernel32.dll", "user32.dll", "gdi32.dll", "advapi32.dll",
                    "shell32.dll", "ole32.dll", "oleaut32.dll", "ws2_32.dll",
                    "crypt32.dll", "secur32.dll", "bcrypt.dll", "ncrypt.dll",
                    "iphlpapi.dll", "winmm.dll", "dbghelp.dll", "psapi.dll",
                    "shlwapi.dll", "setupapi.dll", "cfgmgr32.dll", "dwmapi.dll",
                    "d3d11.dll", "d3d12.dll", "dxgi.dll", "d3dcompiler_47.dll",
                    "mf.dll", "mfplat.dll", "mfreadwrite.dll", "mfuuid.dll",
                    "avrt.dll", "mmdevapi.dll", "dsound.dll", "opengl32.dll",
                    "version.dll", "userenv.dll", "rpcrt4.dll", "comdlg32.dll",
                    "msvcrt.dll", "ntdll.dll", "winhttp.dll", "wldap32.dll",
                    "normaliz.dll", "dnsapi.dll", "imm32.dll", "comctl32.dll",
                    "uxtheme.dll", "hid.dll", "cabinet.dll", "devenum.dll",
                })
    return low in {
        "libc.so.6", "libm.so.6", "libdl.so.2", "libpthread.so.0",
        "librt.so.1", "libgcc_s.so.1", "libstdc++.so.6", "ld-linux-x86-64.so.2",
        "ld-linux-aarch64.so.1", "libresolv.so.2", "libutil.so.1",
        "libgl.so.1", "libgl.so", "libegl.so.1", "libx11.so.6", "libxext.so.6",
        "libdrm.so.2", "libgbm.so.1", "libwayland-client.so.0",
    }


def die(why: str) -> None:
    print(f"gst_trim: {why}", file=sys.stderr)
    sys.exit(1)


def warn(why: str) -> None:
    print(f"gst_trim: warning: {why}", file=sys.stderr)


# Where a prefix keeps gst-plugin-scanner: libexec on the official packages
# and Homebrew, lib on some builds, and Debian's multiarch directory on Ubuntu,
# which is what a hosted Linux runner has and where the first trim there died.
SCANNER_DIRS = (
    "libexec/gstreamer-1.0",
    "lib/gstreamer-1.0",
    "lib/x86_64-linux-gnu/gstreamer1.0/gstreamer-1.0",
    "lib/aarch64-linux-gnu/gstreamer1.0/gstreamer-1.0",
    "lib/gstreamer1.0/gstreamer-1.0",
)


def which_in(prefix: Path, name: str, where: tuple[str, ...] = ("bin", "libexec")) -> Path | None:
    for rel in where:
        for candidate in (prefix / rel / name, prefix / rel / f"{name}.exe"):
            if candidate.is_file():
                return candidate
    return None


def plugin_dir(prefix: Path) -> Path:
    for rel in ("lib/gstreamer-1.0", "lib64/gstreamer-1.0",
                "lib/x86_64-linux-gnu/gstreamer-1.0",
                "lib/aarch64-linux-gnu/gstreamer-1.0", "plugins"):
        if (prefix / rel).is_dir():
            return prefix / rel
    die(f"no gstreamer-1.0 plugin directory under {prefix}")
    raise SystemExit(1)                            # unreachable, for the reader


def library_dirs(prefix: Path, platform: str) -> list[Path]:
    """Where to look for a library named by an import, in order."""
    rels = ["bin", "lib", "lib64", "lib/x86_64-linux-gnu",
            "lib/aarch64-linux-gnu"] if platform == "windows" else \
           ["lib", "lib64", "lib/x86_64-linux-gnu", "lib/aarch64-linux-gnu",
            "bin"]
    dirs = [prefix / r for r in rels if (prefix / r).is_dir()]
    # A Homebrew prefix keeps every dependency in its own cellar, so the
    # sibling opt directories are part of the search. An official framework or
    # an MSVC runtime is self contained and this finds nothing extra.
    cellar = prefix.parent
    if cellar.name == "opt" and cellar.is_dir():
        dirs.extend(p / "lib" for p in cellar.iterdir() if (p / "lib").is_dir())
    return dirs


# ---------------------------------------------------------------- the build --


def resolve(name: str, dirs: list[Path]) -> Path | None:
    """A library named by an import, found in the source prefix."""
    leaf = name.rsplit("/", 1)[-1].rsplit("\\", 1)[-1]
    for d in dirs:
        candidate = d / leaf
        if candidate.is_file():
            return candidate.resolve()
    # Some macOS imports are absolute and outside the prefix entirely, which is
    # what a Homebrew cellar looks like from inside another cellar.
    if name.startswith("/") and Path(name).is_file():
        return Path(name).resolve()
    return None


def closure(seeds: list[Path], dirs: list[Path], platform: str) -> list[Path]:
    """Every library the seeds reach, transitively.

    This is the step that does the real trimming. The libraries in a GStreamer
    runtime outweigh the plugins, and the only honest way to know which ones
    are needed is to ask the files themselves.
    """
    seen: set[Path] = set()
    found: dict[str, Path] = {}
    queue = list(seeds)
    missing: set[str] = set()
    while queue:
        item = queue.pop()
        if item in seen:
            continue
        seen.add(item)
        for name in imports(item, platform):
            if is_system(name, platform):
                continue
            leaf = name.rsplit("/", 1)[-1].rsplit("\\", 1)[-1]
            # A Mach-O lists its own install name first, and a plugin's install
            # name is an absolute path that still exists on this machine. Left
            # alone it copies every plugin into the library directory twice.
            if leaf in found or leaf == item.name:
                continue
            path = resolve(name, dirs)
            if path is None:
                missing.add(leaf)
                continue
            if path.parent.name == "gstreamer-1.0":
                continue
            found[leaf] = path
            queue.append(path)
    for leaf in sorted(missing):
        warn(f"{leaf} is imported but is not in the source prefix; "
             "the machine is expected to have it")
    return sorted(found.values())


def copy(src: Path, dst: Path) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    if dst.exists():
        return
    shutil.copy2(src, dst, follow_symlinks=True)
    dst.chmod(dst.stat().st_mode | 0o644)


def relocate_macos(out: Path) -> None:
    """Make the tree work wherever it is put.

    A Homebrew or framework library records where it was built, and a copy of
    it inside an app would still load the original from the machine, or fail
    on a machine that has no original. Every recorded path is rewritten to be
    relative to the file that carries it, and then the file is signed again,
    because editing a Mach-O breaks the signature and Apple silicon refuses to
    load an unsigned library that claims to be signed.
    """
    def mach_o(p: Path) -> bool:
        if not p.is_file() or p.is_symlink():
            return False
        if p.suffix in (".dylib", ".so"):
            return True
        # The binaries have no suffix at all, so the test is the file's magic:
        # a 64 bit Mach-O, or a fat one carrying both architectures.
        with p.open("rb") as f:
            return f.read(4) in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe")

    files = [p for p in out.rglob("*") if mach_o(p)]
    names = {p.name: p for p in files}
    for path in files:
        args = ["install_name_tool"]
        if path.suffix in (".dylib", ".so"):
            args += ["-id", f"@loader_path/{path.name}"]
        for dep in imports(path, "macos"):
            leaf = dep.rsplit("/", 1)[-1]
            if leaf == path.name or leaf not in names or is_system(dep, "macos"):
                continue
            target = names[leaf]
            rel = os.path.relpath(target, path.parent)
            args += ["-change", dep, f"@loader_path/{rel}"]
        if len(args) > 1:
            subprocess.run(args + [str(path)], capture_output=True)
        subprocess.run(["codesign", "--force", "--sign", "-", str(path)],
                       capture_output=True)


def largest(path: Path, count: int) -> list[Path]:
    files = [p for p in path.rglob("*") if p.is_file()]
    return sorted(files, key=lambda p: p.stat().st_size, reverse=True)[:count]


def megabytes(path: Path) -> float:
    total = sum(p.stat().st_size for p in path.rglob("*") if p.is_file())
    return total / (1024 * 1024)


def wanted_plugins(prefix: Path, plugins: Path, registry: Path,
                   platform: str, codecs: Path) -> tuple[list[str], list[str]]:
    """The plugins to keep, and the elements that asked for them."""
    elements = sorted(catalogue_elements(codecs)
                      | set(PIPELINE) | set(PLATFORM.get(platform, [])))
    where = element_map(prefix, plugins, registry)
    keep = set(ALWAYS_PLUGINS)
    absent = []
    for element in elements:
        plugin = where.get(element)
        if plugin is None:
            absent.append(element)
        else:
            keep.add(plugin)
    return sorted(keep), absent


def build(args: argparse.Namespace) -> int:
    prefix = Path(args.source).resolve()
    out = Path(args.out).resolve()
    platform = args.platform
    if not prefix.is_dir():
        die(f"{prefix} is not a directory")
    plugins = plugin_dir(prefix)

    with tempfile.TemporaryDirectory() as scratch:
        registry = Path(scratch) / "registry.bin"
        keep, absent = wanted_plugins(prefix, plugins, registry, platform,
                                      Path(args.codecs))
        licence: dict[str, str] = {}
        files = []
        copyleft = []
        for name in keep:
            path = plugin_file(prefix, plugins, registry, name, licence)
            lic = licence.get(name, "unknown")
            if path is None:
                warn(f"the registry named plugin {name} but not its file")
            elif args.exclude_gpl and lic.startswith("GPL"):
                print(f"left out because it is {lic}: {name}")
            else:
                files.append(path)
                if lic.startswith("GPL"):
                    copyleft.append(name)

    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    # Plugins first, then the binaries, then the closure of both.
    lib_out = out / ("bin" if platform == "windows" else "lib")
    for path in files:
        copy(path, out / "lib" / "gstreamer-1.0" / path.name)
    seeds = [out / "lib" / "gstreamer-1.0" / p.name for p in files]

    scanner = which_in(prefix, "gst-plugin-scanner", SCANNER_DIRS)
    if scanner is None:
        die("no gst-plugin-scanner in the source prefix; the registry could "
            "never be built")
    copy(scanner, out / "libexec" / "gstreamer-1.0" / scanner.name)
    seeds.append(out / "libexec" / "gstreamer-1.0" / scanner.name)
    for name in KEEP_BINARIES:
        exe = which_in(prefix, name, ("bin",))
        if exe is not None:
            copy(exe, out / "bin" / exe.name)
            seeds.append(out / "bin" / exe.name)

    # The TLS backend is loaded by name at run time, so nothing imports it and
    # the closure cannot find it. Without it an rtmps or https URL fails with
    # "TLS support is not available", which is a confusing way to say a file
    # was left out.
    for rel in ("lib/gio/modules", "lib64/gio/modules"):
        modules = prefix / rel
        if modules.is_dir():
            for mod in modules.iterdir():
                if mod.is_file() and mod.suffix in (".so", ".dll", ".dylib"):
                    copy(mod, out / "lib" / "gio" / "modules" / mod.name)
                    seeds.append(out / "lib" / "gio" / "modules" / mod.name)

    dirs = library_dirs(prefix, platform)
    libs = closure(seeds, dirs, platform)
    for path in libs:
        copy(path, lib_out / path.name)
    # A library can pull in another library once it has been copied next to
    # its own dependencies, so the closure is run again over what landed.
    again = closure([lib_out / p.name for p in libs], dirs, platform)
    for path in again:
        copy(path, lib_out / path.name)

    for path in list(out.rglob("*")):
        if path.is_file() and (path.suffix in NEVER_SUFFIX
                               or any(part in NEVER for part in path.parts)):
            path.unlink()

    if platform == "macos":
        relocate_macos(out)

    size = megabytes(out)
    print(f"{len(files)} plugins, {len(set(libs) | set(again))} libraries")
    if absent:
        print(f"not in this runtime, skipped: {' '.join(absent)}")
    # Said out loud rather than buried in a file listing. An installer that
    # carries a GPL plugin is a GPL installer, and whoever cuts a release has
    # to know which ones travelled. `--exclude-gpl` drops them, and the
    # catalogue falls back to openh264, which is what its licence field is for.
    if copyleft:
        print(f"copyleft plugins in this tree: {' '.join(copyleft)}")
    print(f"{out} is {size:.1f} MB")
    if args.budget_mb and size > args.budget_mb:
        print(f"FAIL over the {args.budget_mb} MB budget by "
              f"{size - args.budget_mb:.1f} MB", file=sys.stderr)
        # What the space went on, so the person deciding what to cut has the
        # numbers in the same log as the failure. The first Windows trim was
        # nineteen megabytes over with nothing to say about where.
        print("the twenty five largest files in the tree:", file=sys.stderr)
        for path in largest(out, 25):
            print(f"  {path.stat().st_size / (1024 * 1024):7.1f} MB  "
                  f"{path.relative_to(out)}", file=sys.stderr)
        return 1
    if args.budget_mb:
        print(f"OK within the {args.budget_mb} MB budget")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--from", dest="source", required=True,
                   help="a GStreamer prefix, already unpacked")
    p.add_argument("--out", required=True, help="where to write the trimmed tree")
    p.add_argument("--platform", required=True,
                   choices=["windows", "macos", "linux"])
    p.add_argument("--codecs", default="codecs.toml",
                   help="the codec catalogue the keep list follows from")
    p.add_argument("--budget-mb", type=float, default=0,
                   help="fail if the tree is bigger than this")
    p.add_argument("--exclude-gpl", action="store_true",
                   help="leave out plugins whose licence is GPL (x264, x265, "
                        "faad). Note that this drops plugins, not libraries: a "
                        "libav built against libx264, which is what most "
                        "distributions ship, still carries it. A licence clean "
                        "build needs a libav built without it as well")
    return build(p.parse_args())


if __name__ == "__main__":
    sys.exit(main())
