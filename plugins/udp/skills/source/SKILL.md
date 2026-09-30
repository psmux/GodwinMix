---
name: udp-source
description: Receive MPEG-TS over UDP or RTP as a GodwinMix source, from a multicast group or a unicast port, choosing one program out of a multiplex. Use when the operator mentions multicast, an IRD, a satellite or cable receiver, an IPTV feed, SMPTE 2022, an address starting udp:// or rtp://, or asks why a UDP source shows no picture or shows the wrong channel.
---

# udp/source

Broadcast plant moves video around as MPEG-TS on multicast groups: an IRD or
an encoder sends to a group such as 239.1.1.1 port 5000, and whatever wants it
joins. This source joins, takes one program, and hands it to the mixer.

## Adding one

```
source.add {id: "sat-news", type: "udp/source", params: {address: "239.1.1.1", port: 5000}}
source.add {id: "ird", type: "udp/source", params: {uri: "udp://@239.10.0.4:1234", program: 1041}}
source.add {id: "encoder", type: "udp/source", params: {port: 5000}}
```

The third is unicast: the sender points at this machine's address, port 5000.
`rtp://` works the same way; RTP and bare TS are told apart by themselves, so
getting the scheme wrong does no harm.

Source specific multicast (`udp://10.0.0.9@232.1.1.1:5000`, or
`source_address`) accepts the group only from that sender. It needs IGMPv3 on
the network.

## Reading its health

The health line says the rate, the address, the packets lost so far, and, when
the feed carries several programs, all of them by number and name with the
chosen one first. `degraded` with "nothing has arrived yet" is almost always
one of three things: the sender is not sending there, a firewall drops UDP, or
the switch does not forward the group to this machine. "stopped N s ago" means
the sender went quiet; the picture comes back by itself when it resumes.

A program that is not in the feed is reported with the list of what is, so
the fix is to read that list and set `program`.

## Things to know

* Loss is counted and never waited for. There is no retransmission in UDP, so
  a lost packet is a damaged picture for a moment, not a stall.
* Nothing listens until the source starts, and the port closes when it stops.
* Two sources on one multicast group and port both receive. Two on one
  unicast port do not: the second is refused and says so.
* `interface` matters only on a machine with more than one network. It is the
  interface name (`eth1`, `en0`).
* Calls: `stats` (datagrams, bytes, loss by kind), `programs` (every program,
  its PIDs, stream kinds and languages).
