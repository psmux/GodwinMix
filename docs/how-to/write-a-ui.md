# Write a UI

Replace the whole interface. Not a panel inside the web UI, not a theme on top
of it: your own program, in your own language, driving the mixer through the
same protocol the first party UI uses. There is no private door. What the web
UI can do, your UI can do.

This page goes from nothing to a first take in TypeScript, Python and Rust, then
shows how to test the result without a mixer running.

## The shape of it

Whatever the language, a surface does the same five things.

1. Open one WebSocket at `/rpc`. A token, if the core wants one, goes in the
   query: a WebSocket cannot set a header.
2. Send `core.subscribe` saying which events you want and which expensive
   streams. Nothing runs on the core unless a client asks for it, so a surface
   that wants no pictures costs the mixer nothing.
3. Apply `event/snapshot`, then the deltas that follow.
4. Repaint at `event/flush` and never per event. A batch of twenty changes is
   then one repaint.
5. Call `program.take` when the operator clicks.

Three libraries do the first four for you. Each is thin, each is generated from
the same `protocol.json`, and each is described in
[the client reference](../reference/clients.md).

## TypeScript

```bash
npm install @godwinmix/client
```

```ts
import { connect } from "@godwinmix/client";

const client = await connect({ base: "http://127.0.0.1:8080", token: "…" });

// Repaint here, not per event.
client.onFlush((state) => {
  for (const source of state.sources) {
    console.log(source.id, source.state, client.store.tallyOf(source.id));
  }
});

await client.settled();                       // the first snapshot has landed
await client.programTake({ source: "cam1" }); // the first take
```

`connect` subscribes on your behalf with the event set most surfaces want. To
choose your own, pass `events`, and ask for the expensive streams separately so
they can be released again:

```ts
const want = client.want("multiview", { fps: 8, width: 960 });
client.on("frame", (frame) => paint(frame.jpeg, frame.layout));
want.release();   // the core stops building the mosaic when the last asker goes
```

Node 22 and later have a global `WebSocket`. On Node 20, pass one:
`connect({ base, token, webSocket: (await import("ws")).WebSocket })`.

## Python

```bash
pip install godwinmix
```

```python
import asyncio, godwinmix

async def main():
    client = await godwinmix.connect("http://127.0.0.1:8080", token="…")

    client.on_flush(lambda state: print(state["program"], len(state["sources"])))

    await client.subscribe(ext={"tally": True})
    await client.settled()

    for source in client.state["sources"]:
        print(source["id"], source["state"], client.tally_of(source["id"]))

    await client.take("cam1")
    await client.close()

asyncio.run(main())
```

Every method in the protocol is a coroutine with named arguments:
`await client.source_add(uri="rtmp://camera/live", name="Camera 1")`,
`await client.output_add(id="twitch", uri="rtmp://…")`. The full list comes
from `protocol.json`, so it grows with the core.

`examples/tkinter-panel.py` is 149 lines of this with widgets on top: a source
list, tally colour, an MJPEG preview and a settings form.

## Rust

```toml
[dependencies]
godwinmix-client = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

```rust
use godwinmix_client::{Client, UI_EVENTS};

#[tokio::main]
async fn main() -> godwinmix_client::Result<()> {
    let client = Client::connect("http://127.0.0.1:8080", Some("…")).await?;
    client.subscribe(&UI_EVENTS, serde_json::json!({"tally": true})).await?;
    client.next_flush().await?;

    for source in &client.state().status.sources {
        println!("{} {} {}", source.id, source.state, client.state().tally_of(&source.id));
    }

    client.take(Some("cam1")).await?;
    Ok(())
}
```

The render loop is a `watch` receiver, so a surface that falls behind coalesces
rather than queueing repaints it no longer needs:

```rust
let mut flushes = client.flushes();
while flushes.changed().await.is_ok() {
    draw(&client.state());
}
```

`cargo run --example take -- http://127.0.0.1:8080 TOKEN cam1` is the whole of a
cut in fifty lines, checks included.

## Any other language

The protocol is JSON-RPC 2.0 over a WebSocket, so anything with a WebSocket can
drive the mixer. `examples/godot/` is a control panel in Godot 4 with
`WebSocketPeer` and no add-on at all, and `dev/smoke_rpc.py` is a session in a
hundred lines of Python standard library. The messages are:

```
 client -> core   {"jsonrpc":"2.0","id":1,"method":"core.subscribe",
                   "params":{"events":["program.*","source.*","flush"],"ext":{}}}
 core -> client   {"jsonrpc":"2.0","id":1,"result":{"seq":41,"events":[…],"ignored_ext":[]}}
 core -> client   {"jsonrpc":"2.0","method":"event/snapshot","params":{"seq":41,"state":{…}}}
 core -> client   {"jsonrpc":"2.0","method":"event/flush","params":{"seq":44}}
 client -> core   {"jsonrpc":"2.0","id":2,"method":"program.take","params":{"source":"cam1"}}
```

`protocol.md` at the repository root lists every method, every event and every
type, and `core.api` answers the same document from a running core.

## Pictures

| Route | What it is | Who it is for |
|---|---|---|
| `GET /api/v1/snapshot/{id}` | one JPEG | an agent, a still, a slow poll |
| `GET /mjpeg/{id}` | `multipart/x-mixed-replace` | Tkinter, Flutter, anything with an HTTP client |
| `POST /whep/{id}` | WebRTC, audio included, under 500 ms | a browser, Flutter, a WebRTC library |
| `ext: {multiview: …}` then binary frames | the mosaic, one decode for every tile | a wall of tiles |

The MJPEG and WHEP routes are specified and not built yet, so a surface written
today should degrade: `godwinmix.preview_stream` in Python reads `/mjpeg` when
the core has it and polls `/api/v1/snapshot` when it does not, and the panel
above never has to know which happened.

A binary WebSocket frame is a 16 byte header then JPEG: `seq` as `u32`, the
layout id as `u32`, the running time in milliseconds as `u64`, all little
endian. The layout id matches `event/multiview.layout`, which is how a client
cuts cells out of a sheet without painting the wrong camera while the grid is
changing. Every library decodes it: `parseFrame`, `parse_frame`,
`godwinmix_client::parse_frame`.

## Settings a UI has never seen

A surface never hardcodes a plugin's settings. Ask the plugin to describe
itself, then render the JSON Schema that comes back. Each library turns a schema
into a description of a form: the fields, their kinds, their units, their
groups, and which of them apply right now.

```python
schema = await client.call("plugin.describe", {"id": "rtmp/output"})
form = godwinmix.describe_form(schema, current_settings)
for field in form.fields:
    print(field.name, field.kind, field.unit, field.visible)
settings = godwinmix.read_form(form, values, touched_secrets)
```

Tkinter gets the widgets for free with `from godwinmix.tk import SchemaForm`.
TypeScript has the same reader as `describeForm` and the reference UI draws the
result as web components. A secret the operator did not retype is left out
rather than blanked, and a field that an `if`/`then` says does not apply is
hidden and not sent.

## Testing without a mixer

Do not mock the client's own transport: that tests the mock. Run a real
WebSocket server in the test and speak the protocol at it. Both first party test
suites do this, and neither needs a dependency to do it.

* TypeScript: `clients/typescript/test/fake-core.ts` is an HTTP server from
  `node:http` that upgrades the socket by hand (the handshake is a SHA-1 of the
  key and a fixed GUID) and frames messages in about forty lines.
* Python: `clients/python/tests/fake_core.py` is the same thing on
  `asyncio.start_server`.
* Rust: `crates/godwinmix-client/tests/fake_server.rs` uses
  `tokio_tungstenite::accept_async`, which is already a dev dependency.

Each of them answers `core.subscribe`, then pushes a snapshot, a delta, a
layout, a binary frame and a flush, which is the sequence a client has to get
right. A test then asserts the state settled, the delta was folded in and the
repaint happened once:

```ts
const fake = await fakeCore({ onSubscribe: serveSession });
fake.answer("core.subscribe", { seq: 41, events: ["*"], ignored_ext: [] });

const client = await connect({ base: fake.url });
const state = await client.settled();
assert.equal(state.program, "cam1");
```

Point the same test at a refusal to see that the error shape survives:

```ts
fake.answer("program.take", {
  error: { code: -32004, message: "no source cam9. This core has cam1 and cam2." },
});
// e.title is "Not found"; e.nextStep is the sentence after the full stop.
```

## Against a real core

When it works against the fake, run it against a real one.
`clients/e2e.sh` starts a core on a free port with a token and no sources, the
way `dev/smoke.sh` does, and runs your command against it:

```bash
clients/e2e.sh node my-ui/smoke.mjs      # $1 is the URL, $2 is the token
```

The three libraries each have one of these under `clients/<language>/e2e/`:
connect, subscribe, snapshot, flush, add a `test://smpte` source, take it, see
`event/program.took`, disconnect. Copy one.

## What to read next

* [The client library reference](../reference/clients.md): what each library
  exposes, and how versions track `api_level`.
* [The protocol reference](../reference/protocol.md): every method, event and
  type.
* [Write a panel](write-a-panel.md): the smaller job, inside the web UI.
* [Use it with an AI agent](use-with-an-ai-agent.md): the same protocol, for a
  program that watches instead of clicking.
