# The client libraries

Three libraries speak the control protocol: `@godwinmix/client` for TypeScript,
`godwinmix` for Python, `godwinmix-client` for Rust. Each is thin on purpose:
connect, subscribe, a state store, typed calls, the frame decoder and the URL
builders. Nothing in any of them can do something a third party client cannot,
because they are all built on the same published contract.

To write a surface with one, read
[Write a UI](../how-to/write-a-ui.md). This page is what each one exposes.

## Versions

The major version of a library is the `api_level` it speaks. All three are at
1.x, which is `api_level` 1.

A core is safe for a library when `api_compatible <= API_LEVEL <= api_level`,
all three numbers coming from `core.info`. A core one level ahead still works:
unknown events reach a listener rather than being dropped, unknown fields are
kept, and a method the library has never heard of can be called by name.

| Library | Package | Version | Runtime dependencies |
|---|---|---|---|
| TypeScript | `@godwinmix/client` | 1.0.0 | none |
| Python | `godwinmix` | 1.0.0 | none (3.10 or later) |
| Rust | `godwinmix-client` | 1.0.0 | tokio, tokio-tungstenite, serde |

## What is generated and what is not

The types, the typed method signatures, the event list and the `ext` table are
written by `clients/gen/generate.py` from `protocol.json`. A method added to
the core reaches all three libraries by running that once, and a test in each
library fails if the committed output and `protocol.json` have parted company.
See [clients/README.md](../../clients/README.md).

Everything else is hand written: the connection, the store, the frame decoder,
the URL builders, the video helpers and the schema reader.

## The shape they share

| Idea | TypeScript | Python | Rust |
|---|---|---|---|
| connect | `connect({base, token})` | `await connect(base, token)` | `Client::connect(base, token)` |
| subscribe | `client.coreSubscribe({events, ext})` | `await client.subscribe(events, ext)` | `client.subscribe(&events, ext)` |
| the state | `client.state` | `client.state` | `client.state()` |
| repaint | `client.onFlush(fn)` | `client.on_flush(fn)` | `client.flushes()` |
| settled | `await client.settled()` | `await client.settled()` | `client.next_flush().await` |
| a typed call | `client.programTake({source})` | `await client.program_take(source=…)` | `client.program_take(&params).await` |
| any call | `client.callAny(method, params)` | `await client.call(method, params)` | `client.call_value(method, params)` |
| an error | `RpcError` | `RpcError` | `Error::Rpc` |
| a frame | `parseFrame(bytes)` | `parse_frame(bytes)` | `parse_frame(&bytes)` |
| an event | `client.on("event", fn)` | `client.on(name, fn)` | `client.events()` |

Method names follow the protocol: `program.take` is `programTake` in
TypeScript and `program_take` in Python and Rust. Convenience aliases exist
where a name is used constantly: `client.take("cam1")` in Python and Rust.

## The state store

The core sends `event/snapshot`, then deltas, then `event/flush`. The store
folds them in and a surface reads from it, never from the wire. Listeners fire
at flush, so a batch of twenty changes repaints once.

What is in it: `program`, `preview`, `sources`, `outputs`, `multiview`, `ad`,
`tally`, `meters`, `alerts`, `layout`, `seq` and `connected`. Meters are merged
outside the flush path because they arrive ten times a second.

`tallyOf(id)` / `tally_of(id)` answers "program", "preview" or "off" from
`event/tally` when the surface asked for it, and works it out from the
programme when it did not, so a surface that declined the stream still colours
its buttons.

## Errors

Every refusal is `{code, message, data}`. The message already names the current
state and the next step, so show it as it came.

| Member | What it gives you |
|---|---|
| `code` | the JSON-RPC code: -32004 not found, -32001 wrong state, -32002 no scope, -32003 safety, -32020 confirm required |
| `message` | the sentence to show |
| `title` | a short heading for a toast: "Not found", "Not ready for that yet" |
| `nextStep` / `next_step` | the sentence after the last full stop |
| `retryable` | whether offering a retry is honest |
| `retryAfterMs` / `retry_after_ms` | how long to wait, when the core said |

A call outstanding when the connection drops fails with a retryable error
rather than hanging. That is tested in all three libraries.

## Asking for the expensive things

Nothing runs on the core unless a client asks. `ext` on `core.subscribe` turns
on the mosaic, the meters, the tally and the rest; the keys and their value
types are in `protocol.json` and re-exported as `EXT_KEYS`.

The TypeScript client counts askers, because a wall of tiles is many things
wanting one stream:

```ts
const want = client.want("multiview", { fps: 8, width: 960 });
want.update({ width: 1280 });
want.release();
```

It subscribes once at the widest width and the highest rate anyone asked for,
coalescing changes over 30 ms, and drops the key entirely when the last asker
releases. Python and Rust pass `ext` to `subscribe` directly: a terminal UI or a
script has one asker, not forty.

## Video

| Helper | TypeScript | Python | Rust |
|---|---|---|---|
| one JPEG | `client.snapshotUrl(id, width)` | `client.snapshot_url(id, width)` | `client.snapshot_url(id, width)` |
| MJPEG | `client.mjpegUrl(id)` | `client.mjpeg_url(id)` | `client.mjpeg_url(id)` |
| WHEP | `attachWhep(video, url)` | `urls.whep(base, id)` | `client.whep_url(id)` |
| the mosaic | `parseFrame`, `cellFor`, `sheetWidthFor` | `parse_frame`, `cell_for`, `sheet_width_for` | `parse_frame`, `Frame::cell`, `sheet_width_for` |

Python adds readers, because a Tkinter panel has no media stack:
`read_mjpeg(url)` yields JPEG bytes, `poll_snapshots(url, every)` does the same
from the snapshot route, and `preview_stream(client, id)` picks whichever the
core has. TypeScript adds `attachWhep`, which is the WHEP handshake over the
platform's WebRTC API and nothing GodwinMix specific.

`/mjpeg` and `/whep` are specified and not built yet. A surface written against
them today should fall back, as `preview_stream` does.

## Schema to a form

`describeForm` / `describe_form` turns a plugin's JSON Schema into a list of
fields: `name`, `label`, `kind`, `unit`, `group`, `required`, `value`,
`choices`, `min`, `max`, `step`, `placeholder` and `visible`. It is data, not
widgets, so a UI draws it in whatever it draws things in.

`kind` is one of `text`, `secret`, `url`, `number`, `integer`, `boolean`,
`choice`, `lines`, `json`.

`readForm` / `read_form` turns the values back into the object to send: hidden
fields left out, empty strings left out, and a secret the operator did not
retype left out rather than blanked.

Understood: objects, scalars, enums, arrays of scalars, `if`/`then` visibility,
`format: "secret"`, `x-gmx-unit` and `x-gmx-group`. Not understood, on purpose:
`$ref` outside `#/$defs/`, `oneOf` discrimination, tuple arrays. A plugin
needing those ships its own editor.

Python ships widgets over it in `godwinmix.tk`, imported separately so a
headless script never loads `tkinter`.

## Testing and end to end runs

Each library is tested against a fake core that speaks the real protocol over a
real socket. The TypeScript and Python fakes are written on the standard
library, so `npm test` and `python3 -m unittest` need nothing installed.

```bash
cd clients/typescript && npm test
python3 -m unittest discover -s clients/python/tests
cargo test -p godwinmix-client

clients/typescript/e2e/run.sh    # each starts a real core on a free port
clients/python/e2e/run.sh
clients/rust/e2e/run.sh
```

## Examples

| Example | What it shows |
|---|---|
| `examples/tkinter-panel.py` | 149 lines: sources, take, tally, preview, a settings form |
| `examples/godot/` | a Godot 4 panel on `WebSocketPeer`, no add-on |
| `examples/ai-director.py` | an agent on the Python library, with the loop from [docs/agents.md](../agents.md) |
| `crates/godwinmix-client/examples/take.rs` | a take from Rust, with the checks around it |
| `dev/smoke_rpc.py` | one session in the Python standard library, no library at all |
