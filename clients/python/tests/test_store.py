"""The store, the frame decoder and the URL builders."""

from __future__ import annotations

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import godwinmix  # noqa: E402
from fake_core import snapshot  # noqa: E402


class StoreTest(unittest.TestCase):
    def store(self) -> godwinmix.Store:
        store = godwinmix.Store()
        store.apply("snapshot", snapshot())
        return store

    def test_snapshot_then_deltas_then_flush(self):
        store = self.store()
        self.assertEqual(store.state["program"], "cam1")
        self.assertFalse(store.apply("source.state", {"source": "cam2", "state": "live"}))
        self.assertEqual(store.source("cam2")["state"], "live")

        store.apply("program.took", {"source": "cam2"})
        self.assertEqual(store.state["program"], "cam2")

        # Only the flush ends a batch, which is when a surface repaints.
        self.assertTrue(store.apply("flush", {"seq": 51}))
        self.assertEqual(store.state["seq"], 51)

    def test_tally_falls_back_to_the_programme(self):
        store = self.store()
        self.assertEqual(store.tally_of("cam1"), "program")
        self.assertEqual(store.tally_of("cam2"), "off")
        store.apply("tally", {"sources": {"cam2": "preview"}})
        self.assertEqual(store.tally_of("cam2"), "preview")

    def test_meters_are_merged_not_replaced(self):
        store = self.store()
        store.apply("meters", {"program": [-12.0, -12.0], "sources": {"cam1": [-18.0]}})
        store.apply("meters", {"program": [-9.0, -9.0], "sources": {"cam2": [-30.0]}})
        self.assertEqual(store.state["meters"]["program"], [-9.0, -9.0])
        self.assertEqual(set(store.state["meters"]["sources"]), {"cam1", "cam2"})

    def test_a_snapshot_keeps_the_streams_beside_the_status(self):
        store = self.store()
        store.apply("meters", {"program": [-12.0], "sources": {}})
        store.apply("alert", {"severity": "warning", "message": "queue is filling"})
        store.apply("snapshot", snapshot(99))
        self.assertEqual(store.state["meters"]["program"], [-12.0])
        self.assertEqual(len(store.state["alerts"]), 1)
        self.assertEqual(store.state["seq"], 99)

    def test_alerts_stack_newest_first_and_stop_at_fifty(self):
        store = self.store()
        for i in range(60):
            store.apply("alert", {"severity": "info", "message": f"alert {i}"})
        self.assertEqual(len(store.state["alerts"]), 50)
        self.assertEqual(store.state["alerts"][0]["message"], "alert 59")

    def test_a_delta_for_a_source_that_is_not_there_is_ignored(self):
        store = self.store()
        self.assertFalse(store.patch_source("nope", {"state": "live"}))


class FrameTest(unittest.TestCase):
    def header(self, seq: int, layout: int, ms: int) -> bytes:
        import struct

        return struct.pack("<IIQ", seq, layout, ms)

    def test_reads_a_frame(self):
        frame = godwinmix.parse_frame(self.header(7, 3, 1234) + b"\xff\xd8\xff\xe0")
        self.assertEqual((frame.seq, frame.layout, frame.running_time_ms), (7, 3, 1234))
        self.assertEqual(frame.jpeg, b"\xff\xd8\xff\xe0")

    def test_a_header_with_no_picture_is_not_a_frame(self):
        self.assertIsNone(godwinmix.parse_frame(self.header(1, 1, 1)))
        self.assertIsNone(godwinmix.parse_frame(b""))

    def test_a_cell_is_only_read_from_the_layout_the_frame_names(self):
        frame = godwinmix.parse_frame(self.header(1, 7, 0) + b"\xff\xd8\xff\xe0")
        layout = {"id": 7, "cells": [{"index": 1, "source": "cam2", "x": 320, "y": 0, "w": 320, "h": 180}]}
        self.assertEqual(godwinmix.cell_for(frame, layout, "cam2")["x"], 320)
        self.assertIsNone(godwinmix.cell_for(frame, dict(layout, id=8), "cam2"))
        self.assertIsNone(godwinmix.cell_for(frame, layout, "cam9"))

    def test_widths_are_rounded_and_clamped(self):
        self.assertEqual(godwinmix.sheet_width_for(320, 3), 960)
        self.assertEqual(godwinmix.sheet_width_for(100, 3), 320)
        self.assertEqual(godwinmix.sheet_width_for(1000, 4), 1920)


class UrlTest(unittest.TestCase):
    def test_schemes_map_across(self):
        self.assertEqual(godwinmix.urls.rpc("http://box:8080"), "ws://box:8080/rpc")
        self.assertEqual(godwinmix.urls.rpc("https://box/", "t"), "wss://box/rpc?token=t")
        self.assertEqual(godwinmix.urls.http_base("wss://box"), "https://box")

    def test_ids_and_tokens_are_escaped(self):
        self.assertEqual(
            godwinmix.urls.snapshot("http://box", "cam 1", 320, "a/b"),
            "http://box/api/v1/snapshot/cam%201?width=320&token=a%2Fb",
        )
        self.assertEqual(godwinmix.urls.mjpeg("http://box", "program"), "http://box/mjpeg/program")
        self.assertEqual(godwinmix.urls.whep("http://box", "program"), "http://box/whep/program")


if __name__ == "__main__":
    unittest.main()
