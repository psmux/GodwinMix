---
name: ndi-source
description: Receive an NDI sender from the studio network as a GodwinMix source, by name or by address, at full or reduced bandwidth. Use when the operator names NDI, a camera on the network, a graphics machine, OBS or vMix on another computer, or asks what cameras are available without naming one.
---

# ndi/source

NDI is how a camera, a graphics machine and a mixer talk to each other on a
studio network without a capture card. A sender announces itself, a receiver
picks it by name, and nothing has an address to type.

## Finding what is there first

```
list_senders {}
```

answers with every sender this machine can see:

```json
{"senders": [{"id": "studio-cam-1", "name": "STUDIO (CAM 1)",
              "address": "10.0.0.21:5961", "width": 1920, "height": 1080, "fps": 30}]}
```

Use the `name` exactly as reported; it is what the sender announces, spaces,
brackets and all.

## Adding one

```
source.add {id: "cam1", type: "ndi/source", params: {name: "STUDIO (CAM 1)"}}
```

For a sender mDNS cannot reach, which means another subnet, a VPN, or a network
where multicast is filtered, give the address instead:

```
params: {address: "10.0.0.21:5961"}
```

## Settings

| Key | Meaning |
|---|---|
| `name` | the NDI name, as announced |
| `address` | `host:port`, when the name cannot be resolved |
| `bandwidth` | `high` for the full picture, `low` for a much smaller one at a fraction of the bandwidth, `audio-only` for sound alone |
| `timestamp_mode` | which clock frames are stamped with. Leave it alone unless lip sync is wrong across several NDI sources |

`low` is the right answer for a source that is only ever in a corner of a
multiview or a picture in picture. It is a real saving, not a small one.

## When there is no runtime

NDI's runtime is not ours to ship: its licence forbids redistribution. Without
it this source refuses to start and the message carries the download page,
https://ndi.video/for-developers/ndi-sdk/. The runtime alone is enough; the SDK
is not needed. An installer that put it somewhere unusual is found through
`NDI_RUNTIME_DIR_V6`.

## When a sender cannot be found

1. Both machines must be on the same subnet. NDI finds senders with mDNS, and
   mDNS does not cross a router. Give the `address` instead.
2. `list_senders` with a `timeout_ms` under 500 usually finds nothing even when
   senders are there; the announcements take about a second to arrive.
3. Some networks filter multicast. Managed switches and guest wifi both do.

## Sending instead

`ndi/output` announces this mixer's programme on the network under a name
everybody else can pick.
