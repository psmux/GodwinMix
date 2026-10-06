"""TEMPORARY: why a plugin in the trimmed Windows tree will not load."""
import ctypes, os, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from gst_trim import imports_pe, is_system

out, src = Path(sys.argv[1]), Path(sys.argv[2])
bin_ = out / "bin"
have = {p.name.lower() for p in bin_.iterdir()}
sysdir = Path(os.environ["SystemRoot"]) / "System32"
os.add_dll_directory(str(bin_.resolve()))
for name in ("gstsrt.dll", "gstdtls.dll", "gstwebrtc.dll", "gstnice.dll", "gstsrtp.dll", "gstrtmp2.dll"):
    plug = out / "lib" / "gstreamer-1.0" / name
    print(f"== {name} exists={plug.is_file()}")
    if not plug.is_file():
        continue
    todo, seen = [plug], set()
    while todo:
        f = todo.pop()
        for dep in imports_pe(f):
            low = dep.lower()
            if low in seen or is_system(dep, "windows"):
                continue
            seen.add(low)
            if low in have:
                todo.append(bin_ / dep)
                s = src / "bin" / dep
                if s.is_file() and s.stat().st_size != (bin_ / dep).stat().st_size:
                    print(f"   {dep}: size {s.stat().st_size} -> {(bin_ / dep).stat().st_size}")
            elif (sysdir / dep).is_file():
                print(f"   from System32: {dep} (wanted by {f.name})")
            else:
                print(f"   MISSING {dep} (wanted by {f.name}); in source bin: {(src / 'bin' / dep).is_file()}")
    try:
        ctypes.WinDLL(str(plug.resolve()))
        print("   loads")
    except OSError as e:
        print(f"   load error: {e}")
