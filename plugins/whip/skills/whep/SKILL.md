---
name: whep-source
description: Watch a WebRTC stream as a GodwinMix source over WHEP, with a bearer token, STUN and TURN. Use when the operator names WHEP, a WebRTC guest, a browser based contributor, a cloud encoder handing back a preview, or gives a URL beginning https that another tool calls a playback or watch endpoint.
---

# whip/whep

WHEP is WHIP pointed the other way: the same single HTTP POST of an SDP offer,
except the media comes back. It is how a browser guest, a cloud service or
another mixer's WHIP output arrives here as a source.

## Adding one

```
source.add {id: "guest", type: "whip/whep",
            params: {endpoint: "https://example.com/whep/guest"}}
```

Then `program.take {source: "guest"}` once `health` is `ok`.

## Settings

| Key | Meaning |
|---|---|
| `endpoint` | the WHEP URL. Required before the source can start |
| `token` | the bearer token, if the service wants one. Stored encrypted, never read back |
| `stun_server` | `stun://host:port`. Usually leave empty |
| `turn_server` | `turn://user:password@host:port`, the relay for a firewall with no direct path |
| `timeout_secs` | how long to wait for the POST to be answered |

## What the health means

`ok` with detail `ice connected` means media is flowing. `degraded` with
`ice checking` means the offer has gone and the media path is not up yet; if it
stays there, there is no path between the two ends and a TURN server is the
fix. `failing` carries the error the endpoint gave, which is usually a 404 (the
stream name is wrong) or a 401 (the token is).

## What it costs

The stream arrives already encoded and is muxed into Matroska for the trip to
the core, which decodes it once on its own hardware aware path. Declared latency
is 100 ms, which is the order of magnitude of a WebRTC jitter buffer; the
element sets the real one per stream.

## Sending instead

`whip/output` is the output in this same plugin.
