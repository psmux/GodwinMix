"""Packaging regressions which do not require an installed GStreamer."""

from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

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


if __name__ == "__main__":
    unittest.main()
