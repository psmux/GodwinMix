"""The client against a fake core."""

from __future__ import annotations

import asyncio
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import godwinmix  # noqa: E402
from fake_core import FakeCore  # noqa: E402


class ClientTest(unittest.TestCase):
    """One event loop per test.

    The client's futures, its flush Event and the fake core's server all belong
    to the loop they were made on, so a test that used a second loop would be
    testing the loop rather than the client.
    """

    def setUp(self) -> None:
        self.loop = asyncio.new_event_loop()
        asyncio.set_event_loop(self.loop)
        self.core = FakeCore()
        self.clients = []

    def run_(self, coro):
        return self.loop.run_until_complete(coro)

    def start(self, session: bool = False) -> FakeCore:
        self.run_(self.core.start())
        self.core.answer("core.subscribe", {"seq": 41, "events": ["*"], "ignored_ext": []})
        if session:
            self.core.on_subscribe = lambda core: core.session()
        return self.core

    def tearDown(self) -> None:
        async def stop():
            for client in self.clients:
                await client.close()
            await self.core.stop()

        self.run_(stop())
        self.loop.close()
        asyncio.set_event_loop(None)

    async def connect(self) -> godwinmix.Client:
        client = await godwinmix.connect(self.core.url, token="t")
        self.clients.append(client)
        return client

    # ------------------------------------------------------------------ tests

    def test_snapshot_then_deltas_then_flush(self):
        self.start(session=True)

        async def go():
            client = await self.connect()
            renders = []
            client.on_flush(lambda state: renders.append(state["seq"]))
            await client.subscribe()
            state = await client.settled(5)
            return client, state, renders

        client, state, renders = self.run_(go())
        self.assertEqual(state["program"], "cam1")
        self.assertEqual(len(state["sources"]), 2)
        # The delta that arrived after the snapshot has been folded in.
        self.assertEqual(client.store.source("cam2")["state"], "live")
        self.assertEqual(state["seq"], 44)
        self.assertEqual(state["layout"]["id"], 7)
        # One repaint for the whole batch, not one per event.
        self.assertEqual(renders, [44])

    def test_the_token_rides_in_the_query(self):
        self.start()

        async def go():
            client = await self.connect()
            await client.subscribe()
            return self.core.calls

        calls = self.run_(go())
        self.assertEqual(calls[0]["method"], "core.subscribe")
        self.assertEqual(calls[0]["params"]["ext"], {})

    def test_nothing_expensive_unless_asked(self):
        self.start()

        async def go():
            client = await self.connect()
            await client.subscribe(ext={"tally": True})
            return self.core.calls[-1]["params"]

        params = self.run_(go())
        self.assertEqual(params["ext"], {"tally": True})
        self.assertIn("flush", params["events"])

    def test_a_typed_call_answers_a_typed_result(self):
        self.start()
        self.core.answer("program.take", lambda p: {"program": p.get("source"), "running_time_ms": 1600})

        async def go():
            client = await self.connect()
            return await client.take("cam2")

        program = self.run_(go())
        self.assertEqual(program["program"], "cam2")
        self.assertEqual(program["running_time_ms"], 1600)

    def test_a_refusal_keeps_its_shape(self):
        self.start()
        self.core.answer(
            "program.take",
            {
                "error": {
                    "code": godwinmix.CODES["NOT_FOUND"],
                    "message": "no source cam9. This core has cam1 and cam2.",
                    "data": {"retryable": False},
                }
            },
        )

        async def go():
            client = await self.connect()
            try:
                await client.take("cam9")
            except godwinmix.RpcError as e:
                return e
            return None

        error = self.run_(go())
        self.assertIsNotNone(error)
        self.assertEqual(error.code, godwinmix.CODES["NOT_FOUND"])
        self.assertFalse(error.retryable)
        self.assertEqual(error.title, "Not found")
        self.assertEqual(error.next_step, "This core has cam1 and cam2.")

    def test_a_call_answers_when_the_core_goes_away(self):
        self.start()
        self.core.silence("program.take")

        async def go():
            client = await self.connect()
            pending = asyncio.ensure_future(client.take("cam1"))
            await asyncio.sleep(0.05)
            self.core.drop()
            try:
                await pending
            except godwinmix.RpcError as e:
                return e
            return None

        error = self.run_(go())
        self.assertIsInstance(error, godwinmix.RpcError)
        self.assertTrue(error.retryable)

    def test_a_binary_frame_arrives_with_its_header_read(self):
        self.start(session=True)

        async def go():
            client = await self.connect()
            seen = self.loop.create_future()
            client.on("frame", lambda frame: seen.done() or seen.set_result(frame))
            await client.subscribe(ext={"multiview": {"fps": 4, "width": 640}})
            return await asyncio.wait_for(seen, 5), client

        frame, client = self.run_(go())
        self.assertEqual(frame.seq, 3)
        self.assertEqual(frame.layout, 7)
        self.assertEqual(frame.running_time_ms, 1500)
        self.assertEqual(frame.jpeg, b"\xff\xd8\xff\xd9")
        # The layout the frame names is the one the store holds, so cutting a
        # cell out of the sheet is safe.
        cell = godwinmix.cell_for(frame, client.state["layout"], "cam2")
        self.assertEqual((cell["x"], cell["w"]), (320, 320))

    def test_an_unknown_event_reaches_a_listener_and_is_not_dropped(self):
        self.start()

        async def go():
            client = await self.connect()
            seen = []
            client.on("event", lambda name, params: seen.append((name, params)))
            await client.subscribe()
            self.core.notify("event/something.new", {"x": 1})
            await asyncio.sleep(0.1)
            return seen

        seen = self.run_(go())
        self.assertIn(("something.new", {"x": 1}), seen)


if __name__ == "__main__":
    unittest.main()
