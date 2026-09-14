# A GodwinMix panel in Godot 4

Open this folder in Godot 4.2 or later and press F5. Type the mixer's address
and token, press Connect, and the sources appear as buttons: click one to take
it. The button tints red for programme and green for preview, and the
programme's picture refreshes every two seconds.

Three files, and no add-on to install:

| File | What it is |
|---|---|
| `project.godot` | the project, pointed at `main.tscn` |
| `main.tscn` | the scene: two fields, a button, a label, a list and a `TextureRect` |
| `main.gd` | the whole client: `WebSocketPeer`, `JSON` and `HTTPRequest` |

## What it shows

The protocol is JSON-RPC over one WebSocket, so an engine with `WebSocketPeer`
is a first class client. `main.gd` is the same sequence the TypeScript, Python
and Rust libraries use:

1. Open `ws://host/rpc?token=…`. A WebSocket cannot set a header, so the token
   goes in the query.
2. Send `core.subscribe` with the event patterns this panel draws and an empty
   `ext`. Nothing expensive runs on the core because nothing asked for it.
3. Apply `event/snapshot`, then the deltas, and redraw at `event/flush` and
   never per event.
4. `program.take` on a click.

The picture comes from `GET /api/v1/snapshot/program` on a timer because a JPEG
into an `ImageTexture` is four lines. Two other routes exist:

* `GET /mjpeg/program` is a `multipart/x-mixed-replace` body; read it with
  `HTTPClient` and scan for the JPEG start and end markers, the way
  `godwinmix.video.read_mjpeg` does in the Python library.
* `POST /whep/program` is WebRTC with audio under 500 ms, and wants a WebRTC
  extension in the project.

Binary WebSocket packets are mosaic frames: a 16 byte header (seq, layout id,
running time in milliseconds, all little endian) then JPEG. `_read_packet`
decodes the header and prints it; a panel that wants the mosaic subscribes with
`ext: {"multiview": {"fps": 4, "width": 640}}` and cuts cells out of the sheet
using `event/multiview.layout`.

## Not run here

This project has not been opened in Godot: the machine it was written on has no
Godot installed. It is deliberately small so it can be checked by reading, and
the protocol half of it matches the tested client libraries line for line. If
something is wrong it will be a scene property or an engine API name, not the
conversation with the mixer.
