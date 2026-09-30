---
name: udp-output
description: Send the GodwinMix programme as MPEG-TS over UDP or RTP, to a multicast group or one receiver, with a TTL, an interface and an optional constant bitrate. Use when the operator wants to feed an IPTV network, a modulator, a hardware decoder, another mixer or VLC on the LAN, mentions multicast out, or gives an address starting udp:// or rtp:// to send to.
---

# udp/output

Puts the programme on the network as MPEG-TS, the way an encoder in a
broadcast rack does. The programme is already encoded by the mixer; this
remuxes it and sends it, and adds no encode.

## Adding one

```
output.add {id: "lan", type: "udp/output", uri: "udp://239.1.1.1:5000"}
output.add {id: "modulator", type: "udp/output", uri: "udp://10.0.0.50:1234", cbr_kbps: 8000}
output.add {id: "iptv", type: "udp/output", uri: "rtp://239.2.2.2:5004", ttl: 16, interface: "eth1"}
```

Anyone on the network then watches with `vlc udp://@239.1.1.1:5000` or
`ffplay udp://239.1.1.1:5000`.

## The settings that matter

* `ttl` (8): how many routers a multicast packet may cross. 1 keeps it on the
  local segment.
* `interface`: the interface multicast leaves by, on a machine with a separate
  media network.
* `cbr_kbps`: pad to a constant rate with null packets, for hardware that
  needs one. Set it 10% or more above the programme's bitrate. 0 is off.
* `packets_per_datagram` (7): 1316 byte datagrams. Change it only when a
  receiver asks.

## Things to know

* UDP has no connection, so health can only say bytes are leaving. Whether
  anyone receives them is for the receiver to say.
* A multicast stream on a network without IGMP snooping reaches every port of
  the switch. On Wi-Fi it can swamp the air. Send multicast on a wired network.
* This output needs a Unix FIFO from the core, so it runs on Linux and macOS.
