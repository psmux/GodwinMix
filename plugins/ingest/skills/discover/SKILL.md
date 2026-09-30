---
name: ingest-discover
description: The channel server. Holds the mixer's RTMP, SRT and RTMPS ports for every channel (each only while a channel uses it) and takes the WHIP publishers the core hands it, lets in the encoders whose keys match, measures each live stream, and hands it to a mixer source or a restream. Use when asked what is publishing right now, over what, at what size, frame rate and bit rate, which ports are open, or why an encoder was turned away.
---

# ingest/discover

One port per protocol, many channels, many streams on each, and no port open
until a channel that is switched on uses it. Channels are the core's:
make and change them with the `channel.*` methods, and the core hands this
device the table. What it does with it:

* a publisher on a channel with a key that matches is let in, and its stream
  becomes a mixer source by itself when the channel's `auto_source` is on;
* one with no key, a wrong key, or on a channel that is off is refused, and
  its encoder is told why in a sentence;
* taking a key back cuts off whoever is on air with it.

With no channels at all nothing listens, unless the plugin's `open_door`
setting is on: then it takes any RTMP publisher, the way it did before
channels, and each becomes a source named after its path.

## streams

```
streams {}
```

answers with every live stream:

```json
{"port": 1935, "relay": "127.0.0.1:1935", "streams": [
  {"app": "sunday-service", "stream": "main", "from": "10.0.0.31:51666", "key": "key-1",
   "video": {"codec": "h264", "width": 1920, "height": 1080, "fps": 30.0, "kbps": 5980},
   "audio": {"codec": "aac", "channels": 2, "sample_rate": 48000, "kbps": 128},
   "readers": 1, "dropped_gops": 0}]}
```

`dropped_gops` counts whole GOPs a slow reader lost. A publisher is never
slowed by a reader; a reader that falls behind loses from the front of its own
queue and picks up again at the next keyframe.

## add_publishers

Only for the open door, with no channels: makes the sources match whoever is
publishing. `add_publishers {dry_run: true}` answers with the plan and changes
nothing. It calls the core's own REST layer with the token in `GMX_TOKEN`, so
that token needs the `operate` scope.

## One port, one holder

This device and an `ingest/rtmp` source that owns its own port cannot share a
port. Give that source another one.
