"""The schema reader: a plugin's settings, without the UI knowing the plugin."""

from __future__ import annotations

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import godwinmix  # noqa: E402

SCHEMA = {
    "type": "object",
    "title": "RTMP output",
    "required": ["url"],
    "properties": {
        "url": {"type": "string", "format": "uri", "title": "Destination", "examples": ["rtmp://live/app"]},
        "key": {"type": "string", "format": "secret", "title": "Stream key"},
        "codec": {"type": "string", "enum": ["h264", "hevc"], "default": "h264"},
        "bitrate": {
            "type": "integer",
            "minimum": 500,
            "maximum": 20000,
            "default": 4500,
            "x-gmx-unit": "kbit/s",
            "x-gmx-group": "Advanced",
        },
        "keyframes": {"type": "number", "multipleOf": 0.5, "x-gmx-group": "Advanced"},
        "hardware": {"type": "boolean", "default": False},
        "hosts": {"type": "array", "items": {"type": "string"}},
        "extra": {"type": "object"},
    },
    "allOf": [
        {"if": {"properties": {"codec": {"const": "hevc"}}}, "then": {"properties": {"keyframes": {}}}},
    ],
}


class DescribeTest(unittest.TestCase):
    def test_every_field_gets_a_kind_a_group_and_a_unit(self):
        form = godwinmix.describe_form(SCHEMA, {"url": "rtmp://live/app"})
        by_name = {f.name: f for f in form.fields}

        self.assertEqual(by_name["url"].kind, "url")
        self.assertTrue(by_name["url"].required)
        self.assertEqual(by_name["url"].placeholder, "rtmp://live/app")
        self.assertEqual(by_name["key"].kind, "secret")
        self.assertEqual(by_name["codec"].kind, "choice")
        self.assertEqual(by_name["codec"].choices, ["h264", "hevc"])
        self.assertEqual(by_name["bitrate"].kind, "integer")
        self.assertEqual(by_name["bitrate"].unit, "kbit/s")
        self.assertEqual(by_name["bitrate"].group, "Advanced")
        self.assertEqual(by_name["bitrate"].minimum, 500)
        self.assertEqual(by_name["hardware"].kind, "boolean")
        self.assertEqual(by_name["hosts"].kind, "lines")
        self.assertEqual(by_name["extra"].kind, "json")
        self.assertEqual(form.groups, ["", "Advanced"])

    def test_the_default_stands_in_for_an_unset_value(self):
        form = godwinmix.describe_form(SCHEMA, {})
        self.assertEqual(form.field("bitrate").value, 4500)

    def test_an_if_then_hides_what_does_not_apply(self):
        form = godwinmix.describe_form(SCHEMA, {"codec": "h264"})
        self.assertFalse(form.field("keyframes").visible)
        godwinmix.apply_conditions(form, {"codec": "hevc"})
        self.assertTrue(form.field("keyframes").visible)


class ReadTest(unittest.TestCase):
    def test_coerces_what_a_text_control_hands_back(self):
        form = godwinmix.describe_form(SCHEMA, {})
        out = godwinmix.read_form(
            form,
            {
                "url": "rtmp://live/app",
                "bitrate": "6000",
                "hardware": True,
                "hosts": "a.example\nb.example\n",
                "extra": '{"x": 1}',
                "codec": "hevc",
            },
        )
        self.assertEqual(out["bitrate"], 6000)
        self.assertEqual(out["hosts"], ["a.example", "b.example"])
        self.assertEqual(out["extra"], {"x": 1})
        self.assertTrue(out["hardware"])

    def test_an_untouched_secret_is_left_out_rather_than_blanked(self):
        form = godwinmix.describe_form(SCHEMA, {"url": "rtmp://x", "key": "already-set"})
        untouched = godwinmix.read_form(form, {"url": "rtmp://x", "key": "•" * 8})
        self.assertNotIn("key", untouched)
        typed = godwinmix.read_form(form, {"url": "rtmp://x", "key": "new-key"}, ["key"])
        self.assertEqual(typed["key"], "new-key")

    def test_a_hidden_field_is_left_out(self):
        form = godwinmix.describe_form(SCHEMA, {"codec": "h264"})
        out = godwinmix.read_form(form, {"url": "rtmp://x", "codec": "h264", "keyframes": "2"})
        self.assertNotIn("keyframes", out)

    def test_missing_names_the_empty_required_fields(self):
        form = godwinmix.describe_form(SCHEMA, {})
        self.assertEqual(godwinmix.missing(form, {}), ["url"])
        self.assertEqual(godwinmix.missing(form, {"url": "rtmp://x"}), [])


class ParityTest(unittest.TestCase):
    """The same schema, read by this library and by @godwinmix/client.

    Two readings of one schema drift. This runs the TypeScript one over the
    same input and compares, so a plugin's settings cannot come out one shape
    in a Tkinter panel and another in the web UI. Skipped when Node is not
    installed or the repository is not around it.
    """

    def test_the_two_readers_agree(self):
        import json
        import shutil
        import subprocess

        root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
        ts = os.path.join(root, "clients", "typescript", "src", "index.ts")
        if not shutil.which("node") or not os.path.exists(ts):
            self.skipTest("node or clients/typescript is not here")

        script = (
            "import { describeForm } from " + json.dumps(ts) + ";\n"
            "const schema = JSON.parse(process.argv[2]);\n"
            "const form = describeForm(schema, { codec: 'h264' });\n"
            "console.log(JSON.stringify(form.fields.map(f => "
            "[f.name, f.kind, f.group, f.required, f.visible, f.unit ?? null])));\n"
        )
        path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "_parity.mjs")
        try:
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(script)
            out = subprocess.run(
                ["node", path, json.dumps(SCHEMA)],
                capture_output=True,
                text=True,
                timeout=60,
            )
        finally:
            if os.path.exists(path):
                os.remove(path)
        if out.returncode != 0:
            self.skipTest(f"node could not run the TypeScript reader: {out.stderr.strip()[:200]}")

        theirs = json.loads(out.stdout)
        form = godwinmix.describe_form(SCHEMA, {"codec": "h264"})
        ours = [[f.name, f.kind, f.group, f.required, f.visible, f.unit] for f in form.fields]
        self.assertEqual(ours, theirs)


if __name__ == "__main__":
    unittest.main()
