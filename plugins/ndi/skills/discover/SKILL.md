---
name: ndi-discover
description: List every NDI sender on the network, with resolution and frame rate, ready to add as a source. Use when the operator asks what cameras or machines are available, names a camera rather than an address, or when an NDI source cannot be found and you need to know what is actually being announced.
---

# ndi/discover

NDI senders announce themselves over mDNS as `_ndi._tcp`. This listens and
reports them.

## The tool

```
list_senders {}
list_senders {timeout_ms: 3000}
```

```json
{"senders": [{"id": "studio-cam-1", "name": "STUDIO (CAM 1)",
              "address": "10.0.0.21:5961", "width": 1920, "height": 1080, "fps": 30}]}
```

`id` is the slug a source would sensibly be added under, so there is no need to
invent one. `name` is what goes in `ndi/source`'s settings, exactly as reported.

A timeout under about 500 ms usually finds nothing on a quiet network even when
senders are there: the announcements take a second or so to arrive.

## `discover`

The same list, as candidates ready for `source.add`, each with type
`ndi/source` and the `name` in its params.

## Nothing found

* Both machines must be on the same subnet; mDNS does not cross a router. Add
  the source by `address` instead.
* Some networks filter multicast, which managed switches and guest wifi both do.
* Without the NDI runtime there is nothing to listen with. `health` says so and
  carries the download page; `discover` answers with an empty list rather than
  an error, because a machine with no NDI on it is not broken.
