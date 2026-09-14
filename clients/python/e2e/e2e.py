#!/usr/bin/env python3
"""One end to end run against a live core.

    python3 clients/python/e2e/e2e.py http://127.0.0.1:8080 TOKEN

Connect, subscribe, receive the snapshot and the flush, add a test source, take
it, see `event/program.took`, disconnect. Every step prints ok, or the script
exits non zero saying what failed.

`clients/python/e2e/run.sh` starts a core the way dev/smoke.sh does and runs
this against it.
"""

from __future__ import annotations

import asyncio
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import godwinmix  # noqa: E402

ID = "e2e-python"


def step(name: str) -> None:
    print(f"{name:<46}", end="", flush=True)


def ok(detail: str = "") -> None:
    print("ok")
    if detail:
        print(f"    {detail}")


def fail(why: str) -> None:
    print("FAIL")
    print(f"    {why}", file=sys.stderr)
    raise SystemExit(1)


async def main() -> None:
    base = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("GMX_URL", "http://127.0.0.1:8080")
    token = sys.argv[2] if len(sys.argv) > 2 else os.environ.get("GMX_TOKEN")

    step("connect to /rpc")
    try:
        client = await godwinmix.connect(base, token)
    except OSError as e:
        fail(str(e))
    ok()

    took: asyncio.Future = asyncio.get_running_loop().create_future()

    def on_took(params):
        if params.get("source") == ID and not took.done():
            took.set_result(params["source"])

    client.on("program.took", on_took)

    step("core.subscribe")
    try:
        result = await client.subscribe(ext={"tally": True})
    except godwinmix.RpcError as e:
        fail(str(e))
    ok(f"ignored ext keys: {result['ignored_ext']}" if result.get("ignored_ext") else "")

    step("event/snapshot then event/flush")
    try:
        state = await client.settled(10)
    except godwinmix.RpcError as e:
        fail(str(e))
    ok(f"flushed at seq {state['seq']}, {len(state['sources'])} sources")

    step("source.add test://smpte")
    try:
        added = await client.source_add(uri="test://smpte", id=ID, name="Python end to end")
    except godwinmix.RpcError as e:
        fail(str(e))
    ok(f"id {added['id']}, state {added.get('state')}")

    step("program.take")
    try:
        program = await client.take(added["id"])
    except godwinmix.RpcError as e:
        fail(f"{e}\n    {e.next_step}")
    ok(f"programme is {program.get('program')}")

    step("event/program.took")
    try:
        await asyncio.wait_for(took, 10)
    except asyncio.TimeoutError:
        fail(f"no event/program.took naming {ID} in ten seconds")
    ok()

    step("a picture, by whichever route this core has")
    try:
        jpeg = next(godwinmix.preview_stream(client, "program", width=320))
        if jpeg[:2] != b"\xff\xd8":
            fail(f"what came back is not a JPEG: {jpeg[:16]!r}")
    except (OSError, StopIteration) as e:
        fail(f"no picture: {e}")
    ok(f"{len(jpeg)} bytes")

    step("source.remove")
    try:
        await client.source_remove(id=added["id"])
    except godwinmix.RpcError as e:
        fail(str(e))
    ok()

    step("disconnect")
    await client.close()
    ok()


if __name__ == "__main__":
    asyncio.run(main())
