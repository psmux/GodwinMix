---
name: whip-output
description: Send the GodwinMix programme to a WHIP endpoint over WebRTC, with a bearer token and automatic reconnection. Use when the operator names WHIP, WebRTC, sub second latency, Cloudflare Stream, Dolby, LiveKit or another service that gave them a URL beginning https and ending in a stream name, or asks why a WHIP output will not connect.
---

# whip/output

WHIP is one HTTP POST carrying an SDP offer. There is no signalling server to
run and no account to make: the service gives you a URL, and often a token, and
that is the configuration.

It buys latency under a second, end to end, which is what makes a two way
conversation possible. It costs reach: WHIP goes to one endpoint, and the
service behind it does the fanning out. To reach an audience directly, RTMP or
SRT to a CDN is still the answer.

## Adding one

```
output.add {id: "away", type: "whip/output",
            params: {endpoint: "https://example.com/whip/studio", token: "..."}}
```

`token` is `format: secret`: it is stored encrypted and never read back. Every
answer this plugin gives says whether a token is set, never what it is.

## Settings

| Key | Meaning |
|---|---|
| `endpoint` | the WHIP URL. Required before the output can start |
| `token` | the bearer token, if the service wants one |
| `stun_server` | `stun://host:port`. Usually leave empty |
| `turn_server` | `turn://user:password@host:port`, the relay for a firewall that blocks direct paths |
| `timeout_secs` | how long to wait for the POST to be answered |
| `reconnect_first_ms`, `reconnect_max_ms` | the backoff, 500 ms doubling to 15 s by default |

## What the health means

| `health` | What is happening |
|---|---|
| `ok`, detail `ice connected` | media is flowing |
| `degraded`, detail `ice checking` | the offer is with the endpoint, the media path is not up yet |
| `degraded`, "refused or dropped" | the endpoint said no, or went away. It is being dialled again |
| `failing` | not connected at all, between attempts |

A session that sits at `ice checking` and never reaches `connected` is almost
always a firewall with no path between the two ends. A TURN server is what fixes
that; nothing in the settings on this side will.

## When it will not connect

1. Check the URL by hand first. A WHIP endpoint answers a POST; a 404 here means
   the stream name is wrong, and a 401 means the token is.
2. `timeout_secs` too low on a slow link makes every attempt look like a
   refusal. 15 seconds is the default for a reason.
3. The programme's video codec must be H.264. WebRTC has no AAC, so the audio is
   decoded and re-encoded to Opus here, but re-encoding the video would be a
   second full encode and this plugin will not do it quietly. If the programme
   is not H.264, the error says so and names `codecs.toml`.

## Receiving instead

`whip/whep` is the source in this same plugin: the same protocol pointed the
other way, for watching a stream somebody else is publishing.
