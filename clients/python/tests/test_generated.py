"""The drift test: godwinmix/_generated.py against protocol.json.

A method added to the core changes protocol.json, and this fails until someone
runs `python3 clients/gen/generate.py` and commits what changed. That is the
whole mechanism by which a core method reaches this library.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
PACKAGE = os.path.dirname(HERE)
ROOT = os.path.dirname(os.path.dirname(PACKAGE))
sys.path.insert(0, PACKAGE)

import godwinmix  # noqa: E402


class GeneratedTest(unittest.TestCase):
    def setUp(self) -> None:
        if not os.path.exists(os.path.join(ROOT, "protocol.json")):
            self.skipTest("installed without the repository around it")

    def test_is_what_the_generator_would_write_today(self):
        run = subprocess.run(
            [sys.executable, os.path.join(ROOT, "clients", "gen", "generate.py"), "--check", "--lang", "python"],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        self.assertEqual(
            run.returncode,
            0,
            "godwinmix/_generated.py is stale. Run `python3 clients/gen/generate.py` and commit the result.\n"
            + run.stdout
            + run.stderr,
        )

    def test_carries_every_method_event_and_ext_key(self):
        with open(os.path.join(ROOT, "protocol.json"), encoding="utf-8") as fh:
            doc = json.load(fh)

        self.assertEqual(godwinmix.API_LEVEL, doc["api_level"])
        self.assertEqual(godwinmix.API_COMPATIBLE, doc["api_compatible"])

        names = {m["name"] for m in godwinmix.METHODS}
        self.assertEqual(names, {m["name"] for m in doc["methods"]})

        for event in doc["events"]:
            self.assertIn(event["pattern"], godwinmix.EVENT_NAMES)

        for ext in doc["ext"]:
            self.assertIn(ext["key"], godwinmix.EXT_KEYS)
            self.assertEqual(godwinmix.EXT_KEYS[ext["key"]]["implemented"], ext["implemented"])

    def test_every_method_is_a_coroutine_on_the_client(self):
        import inspect

        for method in godwinmix.METHODS:
            name = method["name"].replace(".", "_")
            self.assertTrue(hasattr(godwinmix.Client, name), f"Client has no {name}()")
            self.assertTrue(inspect.iscoroutinefunction(getattr(godwinmix.Client, name)))

    def test_knows_which_methods_change_the_mixer(self):
        by_name = {m["name"]: m for m in godwinmix.METHODS}
        self.assertTrue(by_name["program.take"]["mutating"])
        self.assertEqual(by_name["program.take"]["scope"], "operate")
        self.assertEqual(by_name["program.take"]["rest"], ("POST", "/api/v1/program/take"))
        self.assertFalse(by_name["source.list"]["mutating"])


if __name__ == "__main__":
    unittest.main()
