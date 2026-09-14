# See and hear the mixer from anywhere

You have a core running. You want a picture of it, or the sound, in something
that is not the web UI: a Python window, a terminal, a Flutter app, an agent,
your own headphones.

There are four ways in. Pick one from this table and jump to it. None of them
costs the core anything until you open it, and all of them stop the moment you
hang up.

| You want | Use | Delay | Needs |
|---|---|---|---|
| A picture, anywhere, in two lines | [MJPEG](#a-picture-in-two-lines) | under 200 ms on a LAN | an HTTP client |
| Sound, in any language | [PCM over a WebSocket](#sound-in-fifteen-lines) | under 100 ms | a WebSocket client |
| Sound over a slow link | [Opus](#sound-over-a-slow-link) | under 100 ms | a WebSocket client and an Opus decoder |
| Both, smoothly, in a browser | [WHEP](#webrtc-through-whep) | under 500 ms | `whepserversink` installed |
| Raw frames, same machine, no encode | [a local socket](#raw-frames-on-the-same-machine) | one frame | Linux or macOS |

Everything below assumes a core on `http://localhost:8080` with a token in
`$TOKEN`. If your core has no tokens configured, drop the `Authorization`
header and the `?token=` and it all still works.

## A picture in two lines

`/mjpeg/...` answers `multipart/x-mixed-replace`, which is the format every
webcam on earth has used since 1999. Almost everything can read it.

```bash
# The whole mosaic: every camera plus the programme return, one picture.
curl -sN -H "Authorization: Bearer $TOKEN" http://localhost:8080/mjpeg/sheet > sheet.mjpeg

# One camera.
curl -sN -H "Authorization: Bearer $TOKEN" http://localhost:8080/mjpeg/cam1 > cam1.mjpeg
```

In a browser or any HTML you are writing, it is an `img` tag. A browser cannot
set a header, so the token goes in the query:

```html
<img src="http://localhost:8080/mjpeg/program?token=YOUR_TOKEN">
```

In Python with Tkinter and PIL, a live preview window is about fifteen lines:

```python
import io, tkinter as tk, urllib.request
from PIL import Image, ImageTk

url = "http://localhost:8080/mjpeg/program?token=YOUR_TOKEN"
root = tk.Tk(); label = tk.Label(root); label.pack()
stream, buf = urllib.request.urlopen(url), b""

def tick():
    global buf
    buf += stream.read(8192)
    start, end = buf.find(b"\xff\xd8"), buf.find(b"\xff\xd9")
    if start != -1 and end > start:
        frame, buf = buf[start:end + 2], buf[end + 2:]
        label.image = ImageTk.PhotoImage(Image.open(io.BytesIO(frame)))
        label.configure(image=label.image)
    root.after(10, tick)

tick(); root.mainloop()
```

### The five paths

| Path | Shows |
|---|---|
| `/mjpeg/sheet` | the whole mosaic, every source and the programme return |
| `/mjpeg/program` | the programme return on its own |
| `/mjpeg/{source}` | one source, by its id |
| `/mjpeg/preview` | the armed scene. With no scene armed it is the programme tile |
| `/mjpeg/item/{id}` | one scene item as a projector. Answers 501 until the scene server lands |

### Making it bigger

`?width=` asks for a wider picture:

```bash
curl -sN -H "Authorization: Bearer $TOKEN" "http://localhost:8080/mjpeg/cam1?width=640"
```

There is a thing worth knowing here. Every MJPEG stream is cut out of one
mosaic, so a cell is mosaic sized. Asking a cell for `width=640` widens the
mosaic underneath it, which widens it for everybody watching. That is the trade
that makes six people watching six cameras cost one encoder rather than six. For
picking a camera or drawing designer handles the default is plenty; ask for more
only when you need it.

## Sound in fifteen lines

`/pcm/...` is a WebSocket that sends raw audio: 48 kHz stereo, 32 bit float,
10 ms to a frame. Every frame has a 16 byte header in front of it so you can
tell where you are.

Play the programme through your speakers with `sounddevice`:

```python
import numpy as np, sounddevice as sd, websockets.sync.client as ws

out = sd.OutputStream(samplerate=48000, channels=2, dtype="float32")
out.start()
with ws.connect("ws://localhost:8080/pcm/program?token=YOUR_TOKEN") as sock:
    for frame in sock:
        out.write(np.frombuffer(frame[16:], dtype="<f4").reshape(-1, 2))
```

That is the whole thing. Swap `program` for a source id to hear one camera.

### Asking for less

Three query parameters cut the bandwidth. Raw stereo at 48 kHz is about
3 Mbit/s; a mono 16 kHz signed 16 bit stream is 256 kbit/s, which is plenty for
silence detection or a level display.

| Parameter | Values | Default |
|---|---|---|
| `rate` | 8000 to 48000 | 48000 |
| `channels` | 1 or 2 | 2 |
| `format` | `f32` or `s16` | `f32` |

```bash
# What an agent watching for silence would ask for.
ws://localhost:8080/pcm/cam1?rate=16000&channels=1&format=s16
```

### The frame header

16 bytes, little endian, then the samples:

| Offset | Size | What |
|---|---|---|
| 0 | 4 | sequence number, `u32`, counts up from the first frame |
| 4 | 4 | reserved, zero |
| 8 | 8 | running time in nanoseconds, `u64` |

A gap in the sequence means frames were dropped because you were not reading
fast enough. The running times stay continuous either way, so you always know
where a frame belongs.

## Sound over a slow link

`/opus/...` is the same socket with Opus in it: 48 kHz, 20 ms a frame,
64 kbit/s. Same header, same paths, forty times less bandwidth.

```bash
ws://localhost:8080/opus/program?token=YOUR_TOKEN
```

`rate`, `channels` and `format` are ignored here: Opus is a 48 kHz codec, and
the core says so rather than pretending otherwise.

## WebRTC through WHEP

WHEP gives you picture and sound together with under half a second of delay,
which is what you want for a browser, Flutter, or anything with a WebRTC
library.

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/sdp" \
     --data-binary @offer.sdp \
     http://localhost:8080/whep/program
```

WHEP needs the GStreamer element `whepserversink`, which is in the Rust webrtc
plugin. Not every distribution installs it. Ask the core before you build
anything on it:

```bash
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/core/info | grep whep
```

If `whep` is not in the feature list, `POST /whep/...` answers 501 and the
message names the package for your platform. Install it, restart the core, and
try again. Until then `/mjpeg/program` plus `/pcm/program` is the same two
streams with no extra plugin.

### ICE and TURN

A WebRTC connection needs a path between the viewer and the core.

* On a LAN, the addresses both ends already have are enough. Set `stun = ""` and
  nothing leaves your network.
* Across the internet, a STUN server discovers your public address.
* Where both ends are behind an unhelpful NAT, a TURN server relays the media.
  Nothing is relayed unless it has to be, so a TURN server costs bandwidth only
  for the connections that need it.

```toml
[whep]
stun = "stun://stun.l.google.com:19302"
turn = ["turn://user:password@turn.example.com:3478"]
```

## Raw frames on the same machine

If your client runs on the same machine as the core, none of the above is worth
paying for. Ask for a socket and read the frames the mixer already has, with no
encode and no copy.

```bash
# Over the RPC socket, or /rpc, or the CLI.
curl -s -X POST -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"target":"program"}' \
     http://localhost:8080/api/v1/preview/open
# {"target":"program","path":"/home/you/.godwinmix/preview/program.sock","transport":"unixfd"}
```

Read it with `unixfdsrc`:

```bash
gst-launch-1.0 unixfdsrc socket-path=/home/you/.godwinmix/preview/program.sock ! \
    videoconvert ! autovideosink
```

Give it back when you are done:

```bash
curl -s -X POST -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"target":"program"}' \
     http://localhost:8080/api/v1/preview/close
```

`godwinmix --info` prints where these sockets live without your having to open
one first.

This is Linux and macOS. On Windows `preview.open` says so and points you at
`/mjpeg`, which works everywhere.

## What each one costs

Measured on an Apple M4 Pro at 1280x720. Your machine will differ; the ordering
will not.

| Stream | While open | While closed |
|---|---|---|
| `/mjpeg/sheet` | nothing beyond the mosaic it holds up | nothing |
| `/mjpeg/{cell}` | one decode, one crop and one encode per frame | nothing |
| the mosaic itself | about 0.025 of a core at 8 fps, 1280 wide | nothing: no pipeline exists |
| `/pcm/*` | a leaky queue, a convert and a resample | nothing |
| `/opus/*` | the same plus an Opus encoder | nothing |
| `/whep/*` | a video and an audio encoder per session | nothing |
| a local socket | no pixels copied at all | nothing |

"Nothing while closed" is not a figure of speech. Check it:

```bash
curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8080/metrics | grep gmx_stream_clients
# gmx_stream_clients{kind="mjpeg"} 0
# gmx_stream_clients{kind="opus"} 0
# ...
```

See [nothing runs unless asked](../explanation/nothing-runs-unless-asked.md) for
why it is built that way and what else is on that list.

## Which to pick

* Writing a UI in a toolkit that is not a browser: **MJPEG**, plus **PCM** if
  you want sound. Both are a few lines in every language.
* Writing a browser UI: **WHEP** if the element is installed, MJPEG otherwise.
  The web UI takes mosaic frames over its existing `/rpc` socket and needs
  neither.
* Writing an agent: **MJPEG** at a low rate, or a single
  `GET /api/v1/snapshot/{id}` when you only want a look. For sound, **PCM** at
  `rate=16000&channels=1&format=s16`.
* On the same machine as the core: **the local socket**.
* Over a link you are paying for: **Opus**, and MJPEG at a small `width`.

## Reference

[docs/reference/streams.md](../reference/streams.md) has every route, every
parameter, every status code and the exact bytes on the wire.
