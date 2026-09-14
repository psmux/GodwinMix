---
name: ingest-discover
description: Hold one RTMP port for many publishers and report each one as a source ready to add, with the add_publishers tool to add and remove them. Use when several people publish to the mixer at once, when the operator wants sources to appear by themselves as guests connect, or when asked what is publishing right now.
---

# ingest/discover

One address, many publishers. Where `ingest/rtmp` is one source on one port,
this holds the port for everybody and hands each publisher's stream to a source
through a loopback relay.

## What it reports

* `discover` answers with one candidate per publisher, each with `type`
  `ingest/rtmp` and the `params` a `source.add` needs.
* `event/ingest.publisher` goes out the moment somebody connects or leaves,
  carrying `action`, a legible `id`, the `name` (`live/phone`) and the `params`.
* The `add_publishers` tool adds and removes the sources itself.

## add_publishers

```
add_publishers {dry_run: true}
```

answers with what it would do and changes nothing:

```json
{"add": ["live-phone"], "remove": [], "publishers": ["live-phone"]}
```

Without `dry_run` it adds an `ingest/rtmp` source for every publisher that has
none and removes the ones it added whose publisher has gone. It never touches a
source somebody else made. It calls the core's own REST layer with the token in
`GMX_TOKEN`, so it needs that token to carry the `operate` scope; the error says
so if it does not.

## What does not work yet

The core does not start `device` provides, does not call `discover`, does not
read a plugin's `event` notifications, and does not route `tool.call`. Until it
does, the way to take a publisher is an `ingest/rtmp` source that owns its own
port: one source, added once, and every publisher who arrives is live in
seconds. `plugins/ingest/src/device.rs` names the four gaps precisely.

## One port, one holder

This device and an `ingest/rtmp` source cannot both hold 1935. Run one or the
other; the bind error names the other when they clash.
