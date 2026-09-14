# @godwinmix/client

A client for the [GodwinMix](https://github.com/psmux/modulargodwinmix) control
protocol. The same contract the first party UI uses: one WebSocket at `/rpc`,
JSON-RPC over it, `core.subscribe` to say what you want, a snapshot then
deltas, and `event/flush` to say when to repaint.

No runtime dependencies. ESM. Works in a browser and in Node 20 and later.

```bash
npm install @godwinmix/client
```

## A first take

```ts
import { connect } from "@godwinmix/client";

const client = await connect({ base: "http://127.0.0.1:8080", token: "…" });
await client.settled();               // the first snapshot has landed

for (const source of client.state.sources) {
  console.log(source.id, source.name, source.state);
}

await client.programTake({ source: "cam1" });
```

## Rendering

Repaint at flush, never per event. A batch of twenty changes is then one
repaint, which is what keeps a wall of tiles cheap on a Pi.

```ts
client.onFlush((state) => {
  draw(state.sources, state.program, state.tally);
});
```

## Asking for the expensive things

Nothing runs on the core unless a client asks. Pictures, meters and tally are
`ext` streams: ask for one, and release it when whatever wanted it goes away.

```ts
const want = client.want("multiview", { fps: 8, width: 960 });
client.on("frame", (frame) => paint(frame.jpeg, frame.layout));
// the gallery scrolled away
want.release();
```

Several askers are added up: the client subscribes once at the widest width and
the highest rate anyone needs.

## Video

| Helper | What it gives you |
|---|---|
| `client.snapshotUrl("program", 640)` | one JPEG, for a still or an agent |
| `client.mjpegUrl("program")` | `multipart/x-mixed-replace`, an `<img src>` in a browser |
| `attachWhep(video, client.whepUrl("program"))` | WebRTC with audio, under 500 ms |
| `client.want("multiview", …)` plus `on("frame")` | the mosaic, one decode for every tile |

## Settings forms

A surface never hardcodes a plugin's settings. Ask for the schema, and read it
into a description of the form you then draw however you like.

```ts
import { describeForm, readForm } from "@godwinmix/client";

const schema = await client.call("plugin.describe", { id: "rtmp/output" });
const form = describeForm(schema, current);
// form.fields: name, label, kind, unit, group, choices, min, max, visible
const settings = readForm(form, values, touchedSecrets);
```

## Designer kits

The arithmetic a scene surface needs, under `kits`: the record mirror that
applies `event/scene.patch` and suppresses the echo of your own edits, the
prediction ledger that lets a drag draw at input rate, the undo proxy over
`scene.undo`, the canvas geometry, the handles a plugin declares, snapping, the
safe areas, and the UI schema layer that turns a plugin's second document into a
layout.

```ts
import { kits } from "@godwinmix/client";

const handles = kits.handlesFor(box, kits.gizmosFor(plugin.designer));
const grab = kits.hitTest(handles, x, y, 12);
const drag = kits.applyDrag(grab, { box, transform }, dx, dy, { aspect: shift });
await client.call("scene.item.set", { item, props: drag.props });
```

None of it touches a DOM, a socket or a timer, so the same code runs in a
browser, in node and in a worker. The reference implementation is `ui/kits` in
the repository and the Python library has its own copy; all three replay
`ui/kits/fixtures.json` and must answer the same.

## Errors

Every refusal is an `RpcError` with `{code, message, data}`. The message already
names the current state and the next step, so show it as it came.

```ts
try {
  await client.programTake({ source: "cam9" });
} catch (e) {
  toast(e.title, e.message);        // "Not found", then the sentence
  if (e.retryable) offerRetry(e.retryAfterMs);
}
```

## Node 20

Node 22 and later have a global `WebSocket`. On Node 20 there is none, so pass
one in:

```ts
import WebSocketImpl from "ws";
const client = await connect({ base, token, webSocket: WebSocketImpl });
```

## Versions

The major version is the `api_level` this package speaks. A core is safe when
its `core.info` reports `api_compatible <= API_LEVEL <= api_level`.

## Development

```bash
npm test                    # Node's test runner, against a fake core, no network
npm run build               # tsc to dist/
python3 ../gen/generate.py  # regenerate src/generated/protocol.ts from protocol.json
./e2e/run.sh                # start a real core and run one end to end session
```

`src/generated/protocol.ts` is written by `clients/gen/generate.py` from
`protocol.json` and a test fails if the two have parted company. See
`clients/README.md`.
