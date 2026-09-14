# whip

WebRTC in and out of GodwinMix, over HTTP.

WHIP is one HTTP POST carrying an SDP offer. That is the whole protocol. There
is no signalling server for anyone to run, which is why a browser, a phone or a
cloud service can take a stream from this mixer with nothing but a URL and
sometimes a token. WHEP is the same thing pointed the other way, so this plugin
carries both:

| Provide | What it does |
|---|---|
| `whip/output` | sends the programme to a WHIP endpoint |
| `whip/whep` | receives somebody else's stream as a source |

WebRTC buys latency under a second, end to end, which is what makes a two way
conversation work. It costs reach: WHIP goes to one endpoint and the service
behind it does the fanning out. To reach an audience directly, RTMP or SRT to a
CDN is still the answer.

## In four minutes

```sh
dev/harness/stage-plugins.sh
gmx plugin add ./plugins/whip
gmx output add away --type whip/output \
  --params '{"endpoint":"https://example.com/whip/studio","token":"..."}'
```

and to watch a stream somebody else is publishing:

```sh
gmx source add guest --type whip/whep \
  --params '{"endpoint":"https://example.com/whep/guest"}'
```

`token` is `format: secret`. It is stored encrypted, never returned, and no
answer this plugin gives says more than whether one is set.

## What the core cannot do yet

`whip/output` is written to the output contract in
`docs/reference/plugin-lifecycle.md` and it is complete. It is **not reachable
through `output.add` today**, because the core's output registry
(`crates/godwinmix-core/src/plugin/output.rs`) is a static list of the built in
outputs and does not consult the plugin loader, the way `plugin/source.rs`
already does for sources. One `.or_else(|| loader::output_provide(type_id))`
there is what it needs. Until then:

* `whip/whep` works end to end, because sources do go through the loader.
* `whip/output` passes `gmx plugin test plugins/whip --offline` and the
  handshake and configure checks of the full harness, and can be driven by hand
  against a FIFO. It cannot yet be added to a running mixer.

Sidecar outputs are also Unix only in the core as it stands: the programme
reaches an output plugin on a FIFO, and the core refuses to make one on
Windows, naming `rtmp/output` and `srt/output` as the ones that work
everywhere. This plugin compiles on Windows and will work there when the core
grows a named pipe.

## The pipelines

```
whip/output   filesrc(the core's FIFO) ─► matroskademux ─┬─► h264parse ──────────────► whipclientsink
                                                         └─► decode ! convert ! resample ┘

whip/whep     whepsrc ─► matroskamux ─► fdsink fd=1
```

The programme's video is passed through: `whipclientsink` takes `video/x-h264`
on its request pad, so the encode the core already did is the only encode. The
audio is not: WebRTC has no AAC, so it is decoded and handed over raw for the
sink to make Opus of. One audio decode is the price of the protocol, not of this
plugin. If the programme's video is not H.264 the error says so and names
`codecs.toml`, rather than quietly adding a second full video encode.

On the WHEP side, `whepsrc` is preferred over its replacement `whepclientsrc`
because it hands over the encoded streams: they cross to the core as they
arrived and are decoded once, there. `whepclientsrc` decodes inside itself,
which costs a decode here and a much fatter pipe. GStreamer 1.28 prints a
deprecation warning for `whepsrc`; until the cheap one is actually gone it is
the one to use, and this plugin falls back to the other automatically.

## Settings

Both provides read the same keys, because the two protocols are the same
protocol.

| Key | Default | What it does |
|---|---|---|
| `endpoint` | empty | the WHIP or WHEP URL. Required before start |
| `token` | empty | the bearer token. `format: secret` |
| `stun_server` | empty | `stun://host:port`. Usually leave empty |
| `turn_server` | empty | `turn://user:password@host:port`, the relay for a firewall with no direct path |
| `timeout_secs` | `15` | how long to wait for the POST to be answered |
| `reconnect_first_ms` | `500` | output only: the first wait before dialling again |
| `reconnect_max_ms` | `15000` | output only: where the doubling stops |

The reconnect ceiling is deliberately low. An output that is down holds the
core's outage buffer open and that buffer is finite, so waiting five minutes
between attempts would trade a short outage for a long one.

An endpoint may legally be empty at `initialize`: `configure` before `start` is
part of the protocol and is how the first real params often arrive. It is
`start` that refuses, naming the field.

## Health

| `health` | What is happening |
|---|---|
| `ok`, detail `ice connected` | media is flowing |
| `degraded`, `ice checking` | the offer is with the endpoint, the media path is not up |
| `degraded`, "refused or dropped" | the endpoint said no or went away; dialling again |
| `failing` | not connected at all, between attempts |

A session that sits at `ice checking` and never reaches `connected` means there
is no path between the two ends. A TURN server fixes that; nothing else here
will.

## Testing it

```sh
cargo test -p gmx-whip                     # unit tests; they skip where the elements are missing
gmx plugin test plugins/whip --offline     # replay tests/transcript.jsonl, no core, no network
gmx plugin test plugins/whip               # the full harness
```

The full harness picks the first `source` provide, which is `whip/whep`, and
its frame counting checks need something actually publishing to a WHEP
endpoint. With no endpoint they fail honestly: a source that receives nothing
produces no frames. The offline replay needs neither.

## Where the rules come from

`docs/reference/plugin-manifest.md`, `docs/reference/plugin-protocol.md` and
`docs/reference/plugin-lifecycle.md`. The how to page is
`docs/how-to/send-to-whip.md`.
