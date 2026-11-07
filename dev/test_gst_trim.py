"""Packaging regressions which do not require an installed GStreamer."""

from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

import gst_symbols
import gst_trim


class LibraryNames(unittest.TestCase):
    def test_a_versioned_library_keeps_the_name_the_loader_requests(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            real = root / "libsample.so.1.2.3"
            real.write_bytes(b"library")
            alias = root / "libsample.so.1"
            alias.symlink_to(real.name)
            found = gst_trim.resolve(alias.name, [root])
            self.assertEqual(found, alias)
            out = root / "bundle" / found.name
            gst_trim.copy(found, out)
            self.assertEqual(out.read_bytes(), b"library")
            self.assertEqual(out.name, "libsample.so.1")

    def test_transitive_aliases_are_kept_without_rewalking_the_same_file(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            seed = root / "plugin.so"
            seed.touch()
            real = root / "libsample.so.1.2"
            real.touch()
            alias = root / "libsample.so.1"
            alias.symlink_to(real.name)
            calls = []

            def imports(path, platform):
                calls.append(path)
                return [alias.name] if path == seed else [real.name]

            with patch.object(gst_trim, "imports", imports):
                found = gst_trim.closure([seed], [root], "linux")
            self.assertIn(alias, found)
            self.assertEqual(len(calls), 2)


def pe(symbols: int, section: bytes) -> bytes:
    """The headers of a PE file with one section, which is all the debug
    test reads."""
    data = bytearray(0x200)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x80 + 6, 1)
    struct.pack_into("<II", data, 0x80 + 12, 0x180 if symbols else 0, symbols)
    struct.pack_into("<H", data, 0x80 + 20, 0xF0)
    head = 0x80 + 24 + 0xF0
    data[head:head + len(section)] = section
    return bytes(data)


class Stripping(unittest.TestCase):
    def test_an_msvc_dll_is_left_alone(self):
        self.assertFalse(gst_symbols.pe_has_debug(pe(0, b".text")))

    def test_a_mingw_dll_with_symbols_or_dwarf_is_stripped(self):
        self.assertTrue(gst_symbols.pe_has_debug(pe(12, b".text")))
        self.assertTrue(gst_symbols.pe_has_debug(pe(0, b".debug_i")))
        self.assertTrue(gst_symbols.pe_has_debug(pe(0, b"/4")))


class HardwarePlugins(unittest.TestCase):
    def test_a_gpu_plugin_is_kept_by_file_on_a_machine_with_no_gpu(self):
        # nvcodec and va register no element without the GPU, so the build
        # machine's registry never names them. Their files decide.
        with tempfile.TemporaryDirectory() as folder:
            plugins = Path(folder)
            for leaf in ("libgstva.so", "libgstnvcodec.so", "libgstx264.so"):
                (plugins / leaf).write_bytes(b"plugin")
            self.assertEqual(gst_trim.hardware_plugins(plugins, "linux"), {"va", "nvcodec"})
            self.assertEqual(gst_trim.hardware_plugins(plugins, "macos"), set())

    def test_windows_finds_its_plugins_under_their_dll_names(self):
        with tempfile.TemporaryDirectory() as folder:
            plugins = Path(folder)
            for leaf in ("gstnvcodec.dll", "gstqsv.dll", "gstamfcodec.dll"):
                (plugins / leaf).write_bytes(b"plugin")
            self.assertEqual(gst_trim.hardware_plugins(plugins, "windows"),
                             {"nvcodec", "qsv", "amfcodec"})


if __name__ == "__main__":
    unittest.main()
