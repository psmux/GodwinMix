---
name: ingest-rtmp
description: Take an RTMP stream published to the mixer, so a phone, an OBS on another machine or a hardware encoder can send to it with nothing configured. Use when the operator asks how someone sends video to this mixer, mentions a stream key, OBS, a phone app, a remote guest or a hardware encoder, or asks for the address to give somebody.
---

# ingest/rtmp

The mixer waits and the publisher dials in. This is the opposite of the built in
`rtmp/source`, which dials out to somebody else's server, and it is how a church
or a small studio actually receives pictures.

## Adding one

```
source.add {id: "phone", type: "ingest/rtmp"}
```

That is the whole configuration. It listens on port 1935 and takes the first
publisher who arrives, whatever application name and stream key they use. Tell
them:

```
Server:     rtmp://<the mixer's address>:1935/live
Stream key: anything
```

In OBS that is Settings, Stream, Custom, those two fields. On a phone app it is
usually one box that takes the whole URL: `rtmp://<address>:1935/live/phone`.

The picture appears within a second or two of the publisher's first keyframe,
and `health` turns from `degraded` to `ok`. Then `program.take {source: "phone"}`.

## Settings

| Key | Default | What it does |
|---|---|---|
| `port` | `1935` | the TCP port. 1935 is what every encoder fills in by itself |
| `bind` | `0.0.0.0` | every interface. Give one address to listen on that one only |
| `app` | empty | the first part of the publish path. Empty takes any |
| `stream_key` | empty | the rest of it. Empty takes any. Setting one is the closest RTMP has to a password |
| `relay` | empty | filled in by `ingest/discover`. Leave it alone |

## Two publishers

One source is one picture, so a second publisher is refused while the first is
live, with a message saying so. To take two at once, add a second
`ingest/rtmp` source on another port (1936), or run `ingest/discover`, which
holds one port for many publishers.

## When nothing arrives

1. `health` says nobody is publishing and prints the address to use. Read it
   back to the person publishing, port and all.
2. The mixer's address is the one on the network, not `localhost`. A phone
   cannot reach `127.0.0.1`.
3. A firewall between them. RTMP is TCP on 1935.
4. A `stream_key` set here refuses anybody using a different one. The refusal
   reaches the publisher's own error message.
5. Below 1024, binding fails: that is the operating system, not the mixer.

## The other directions

`rtmp/output` sends the programme to somebody else's RTMP server, and
`rtmp/source` pulls from one. Both are built into the core.
