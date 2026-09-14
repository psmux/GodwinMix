# Send the programme to a WHIP endpoint

WHIP is one HTTP POST carrying an SDP offer. There is no signalling server for
anybody to run and no account to make: a service gives you a URL, often a token,
and that is the configuration.

It buys latency under a second, end to end, which is what makes a two way
conversation work. It costs reach: WHIP goes to one endpoint, and the service
behind it does the fanning out. To reach an audience directly, RTMP or SRT to a
CDN is still the answer.

This page takes about five minutes.

## What the core cannot do yet

`whip/output` is complete against the output contract and **is not reachable
through `output.add` today**. The core's output registry
(`crates/godwinmix-core/src/plugin/output.rs`) is a static list of the built in
outputs and does not consult the plugin loader, the way `plugin/source.rs`
already does for sources. One `.or_else(|| loader::output_provide(type_id))`
there is what it needs.

So this page describes what the plugin does and how it is configured, and the
commands under "Add the output" will answer "no such output type" until that
lands. Nothing else on this page is waiting on anything: `whip/whep`, the source
half, works now.

Sidecar outputs are also Unix only in the core as it stands. The programme
reaches an output plugin on a FIFO, and the core refuses to make one on Windows,
naming `rtmp/output` and `srt/output` as the ones that work everywhere.

## Install the plugin

```sh
dev/harness/stage-plugins.sh
gmx plugin add ./plugins/whip
```

## Add the output

```sh
gmx output add away --type whip/output \
  --params '{"endpoint":"https://example.com/whip/studio","token":"..."}'
```

`token` is `format: secret`: stored encrypted, never returned. Every answer this
plugin gives says whether a token is set, never what it is.

## Read the connection

```sh
gmx ctl status
```

| Health | What is happening |
|---|---|
| `ok`, detail `ice connected` | media is flowing |
| `degraded`, `ice checking` | the offer is with the endpoint, the media path is not up yet |
| `degraded`, "refused or dropped" | the endpoint said no or went away; it is being dialled again |
| `failing` | not connected at all, between attempts |

A session that sits at `ice checking` and never reaches `connected` means there
is no path between the two ends. A TURN relay fixes that and nothing else here
will:

```json
{"turn_server": "turn://user:password@turn.example.com:3478"}
```

## Reconnection

When the endpoint drops, the output waits 500 ms and dials again, doubling to 15
seconds. The ceiling is deliberately low: an output that is down holds the core's
outage buffer open, and that buffer is finite, so waiting five minutes between
attempts would trade a short outage for a long one.

```json
{"reconnect_first_ms": 500, "reconnect_max_ms": 15000}
```

## What it costs

The programme's video is passed straight through. `whipclientsink` takes
`video/x-h264` on its request pad, so the encode the core already did is the
only encode. The audio is not: WebRTC has no AAC, so it is decoded and handed
over raw for the sink to make Opus of. One audio decode is the price of the
protocol, not of the plugin.

If the programme's video is not H.264 the plugin refuses with a message naming
`codecs.toml`, rather than quietly adding a second full video encode to every
show.

## Receive a WHEP stream instead

The same plugin carries the source half: WHEP is WHIP pointed the other way,
and it works today.

```sh
gmx source add guest --type whip/whep \
  --params '{"endpoint":"https://example.com/whep/guest"}'
gmx take guest
```

A WHEP session is negotiated while the source starts, so an endpoint that is not
there fails immediately with a message naming the URL, and the core's supervisor
restarts the source with its own backoff.

## Settings, both provides

| Key | Default | What it does |
|---|---|---|
| `endpoint` | empty | the WHIP or WHEP URL. Required before start |
| `token` | empty | the bearer token. `format: secret` |
| `stun_server` | empty | `stun://host:port`. Usually leave empty |
| `turn_server` | empty | `turn://user:password@host:port` |
| `timeout_secs` | `15` | how long to wait for the POST to be answered |
| `reconnect_first_ms` | `500` | output only |
| `reconnect_max_ms` | `15000` | output only |

An endpoint may be empty at `initialize`: `configure` before `start` is part of
the protocol and is how the first real params often arrive. It is `start` that
refuses, naming the field.

## When it will not connect

1. Check the URL by hand. A WHIP endpoint answers a POST; a 404 means the stream
   name is wrong and a 401 means the token is.
2. `timeout_secs` too low on a slow link makes every attempt look like a
   refusal. 15 seconds is the default for a reason.
3. On an older GStreamer the elements are missing. `whipclientsink` and
   `whipserversrc` are in the 1.28 rs webrtc set; `whepsrc` came earlier. The
   plugin says which one is absent and where it comes from.

## Where to go next

* [Receive a phone or an OBS stream](receive-a-phone-or-obs-stream.md), which
  includes running a WHIP endpoint on the mixer so a browser can publish to it
* [Receive and send SRT](srt.md)
* [The network plugins, every setting](../reference/plugins-network.md)
