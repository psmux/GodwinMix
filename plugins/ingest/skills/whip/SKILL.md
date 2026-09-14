---
name: ingest-whip
description: Run a WHIP endpoint on the mixer so a browser can publish to it over WebRTC with nothing but a URL and no app to install. Use when the operator wants a guest to join from a web page, mentions WebRTC or WHIP for an incoming stream, asks for sub second latency from a contributor, or wants something a phone browser can use without an app.
---

# ingest/whip

The mixer runs the WHIP server. A guest opens a page, allows the camera, and
publishes: there is no app to install and no account to make, because WHIP is
one HTTP POST carrying an SDP offer.

## Adding one

```
source.add {id: "guest", type: "ingest/whip"}
```

It listens on port 8889 at `/whip`. Give the guest
`http://<the mixer's address>:8889/whip` and any WHIP publisher, including a
browser page using `RTCPeerConnection`, can send to it.

## Settings

| Key | Default | What it does |
|---|---|---|
| `port` | `8889` | the port the endpoint's HTTP server listens on |
| `bind` | `0.0.0.0` | every interface |
| `path` | `/whip` | the part of the URL after the host, beginning with a slash |
| `stun_server` | empty | `stun://host:port`. Not needed on a local network |

## When nobody arrives

`health` stays `degraded` and prints the address. Check it is the machine's
network address rather than `localhost`, and that the port is open. Across the
internet a STUN server is usually needed so both ends learn their public
addresses; behind a strict firewall, a TURN relay is what fixes it, and no
setting on this side will.

## If the element is missing

`whipserversrc` arrived in the GStreamer 1.28 rs webrtc set. On an older build
this source refuses to start with a message naming the package. Take the stream
over RTMP with `ingest/rtmp` instead, whose protocol is written in Rust and
needs no webrtc element.
