---
name: srt-source
description: Receive an SRT stream as a GodwinMix source, in listener or caller mode, with a latency budget, a passphrase and a stream id. Use when the operator says SRT, mentions a hardware encoder, a contribution feed, a remote guest on a bad connection, or an address beginning srt://, or asks why an SRT source shows no picture.
---

# srt/source

SRT carries an encoded stream over a link that loses packets. It asks for lost
packets again inside a delay budget you choose, so a 300 ms budget survives a
lot of a bad hotel connection. A larger budget is steadier and adds exactly that
much delay. Nothing else trades off.

## Adding one

```
source.add {id: "guest", type: "srt/source", params: {mode: "listener", port: 9000, latency_ms: 300}}
```

The person sending then points their encoder at `srt://<this machine>:9000` in
caller mode. That is the usual direction: the mixer waits, the encoder dials.

To dial out instead, when the far end is already listening:

```
source.add {id: "feed", type: "srt/source",
            params: {mode: "caller", host: "203.0.113.10", port: 9000, latency_ms: 300}}
```

An address pasted from a hardware encoder goes in whole and wins over `host` and
`port`:

```
params: {uri: "srt://203.0.113.10:9000?mode=caller&latency=300&streamid=live/cam1"}
```

## Settings

| Key | Meaning |
|---|---|
| `mode` | `listener` (wait), `caller` (dial out), `rendezvous` (both, through a firewall) |
| `host`, `port` | the interface to wait on, or the sender to dial. The port is UDP |
| `uri` | a whole `srt://` address; wins over `host` and `port` |
| `latency_ms` | the delay budget, 0 to 10000. 125 on a LAN, 200 to 500 on the internet |
| `passphrase` | 10 to 79 characters, the same at both ends. Stored encrypted, never read back |
| `stream_id` | the name the sender publishes under, where the far end sorts by it |
| `auto_reconnect` | dial again by itself when the link drops |

## Reading the health

`health` answers `ok` with the link numbers once packets are arriving:
`rtt 14.2 ms, 31 lost, 4 retransmitted`. Before any packet arrives it answers
`degraded` with the reason. The same numbers come back from a `stats` call.

A steadily rising `retransmitted` with a stable picture means the latency budget
is doing its job. A rising `lost` means the budget is too small for the link:
raise `latency_ms` and reload.

## When there is no picture

1. `health` says "no packets have arrived yet". Nothing has reached the port.
   Check the sender is running, then that the port is open. SRT is UDP.
2. The passphrase must match exactly at both ends, or the handshake fails with
   no packets and no error a receiver can see. This is the usual cause.
3. In listener mode with a `stream_id` set, a sender publishing under a
   different id is refused. Clear `stream_id` to accept any.
4. `mode` must be the opposite of the far end's. Two listeners never meet.

## Sending, not receiving

`srt/output` is built into the core; it is not this plugin and does not need
installing. `output.add {id: "away", type: "srt/output", params: {uri: "srt://host:9000"}}`.
