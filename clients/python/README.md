# godwinmix

A client for the [GodwinMix](https://github.com/psmux/modulargodwinmix) control
protocol. The same contract the first party UI uses: one WebSocket at `/rpc`,
JSON-RPC over it, `core.subscribe` to say what you want, a snapshot then
deltas, and `event/flush` to say when to repaint.

Nothing but the standard library at runtime. Python 3.10 or later.

```bash
pip install godwinmix
```

## A first take

```python
import asyncio, godwinmix

async def main():
    client = await godwinmix.connect("http://127.0.0.1:8080", token="…")
    await client.subscribe(ext={"tally": True})
    await client.settled()

    for source in client.state["sources"]:
        print(source["id"], source["state"], client.tally_of(source["id"]))

    await client.take("cam1")
    await client.close()

asyncio.run(main())
```

## Rendering

Repaint at flush, never per event. A batch of twenty changes is then one
repaint.

```python
client.on_flush(lambda state: draw(state["sources"], state["program"]))
```

Every event also reaches a listener under its own name:

```python
client.on("alert", lambda params: toast(params["severity"], params["message"]))
client.on("frame", lambda frame: paint(frame.jpeg, frame.layout))
```

## Asking for the expensive things

Nothing runs on the core unless a client asks. The mosaic, the meters and the
tally are `ext` streams:

```python
await client.subscribe(ext={"tally": True, "multiview": {"fps": 4, "width": 640}})
```

## Pictures

```python
for jpeg in godwinmix.preview_stream(client, "program", width=640):
    show(jpeg)      # PIL.Image.open(io.BytesIO(jpeg))
```

`preview_stream` reads `/mjpeg/{name}` where the core has it and falls back to
polling `/api/v1/snapshot/{name}` where it does not, so a panel written today
picks up the live stream the day the core grows it. Both are blocking and
belong on a thread of their own.

## Settings forms

A surface never hardcodes a plugin's settings. Ask for the schema, read it, and
draw it however your toolkit draws things:

```python
schema = await client.call("plugin.describe", {"id": "rtmp/output"})
form = godwinmix.describe_form(schema, current)
for field in form.fields:
    print(field.name, field.kind, field.unit, field.visible)
settings = godwinmix.read_form(form, values, touched_secrets)
```

Tkinter gets the widgets for free:

```python
from godwinmix.tk import SchemaForm
form = SchemaForm(parent, schema, current)
form.grid(row=0, column=0, sticky="ew")
...
await client.call("plugin.configure", {"id": plugin, "settings": form.read()})
```

`godwinmix.tk` is a separate import on purpose: a headless script never loads
`tkinter`.

## Errors

Every refusal is an `RpcError` with `code`, `message` and `data`. The message
already names the current state and the next step.

```python
try:
    await client.take("cam9")
except godwinmix.RpcError as e:
    print(e.title, e.message)      # "Not found", then the sentence
    if e.retryable:
        await asyncio.sleep((e.retry_after_ms or 1000) / 1000)
```

## Versions

The major version is the `api_level` this package speaks. A core is safe when
its `core.info` reports `api_compatible <= API_LEVEL <= api_level`.

## Development

```bash
pip install -e clients/python
python3 -m unittest discover -s clients/python/tests   # against a fake core
python3 clients/gen/generate.py                        # regenerate _generated.py
./clients/python/e2e/run.sh                            # one run against a real core
```

`godwinmix/_generated.py` is written by `clients/gen/generate.py` from
`protocol.json`, and a test fails if the two have parted company. See
`clients/README.md`.
