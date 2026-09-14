# srt

Receive SRT as a GodwinMix source.

SRT is what you use when the link between the camera and the mixer loses
packets: it asks for the lost ones again, inside a delay budget you choose. A
300 ms budget rides out a lot of a bad hotel connection. A bigger budget is
steadier and adds exactly that much delay to the programme. That is the only
trade in the protocol, and `latency_ms` is where you make it.

Sending over SRT is **not** in this plugin. `srt/output` is built into the core
at tier 0 and stays there: the programme is already encoded on the output tee,
so a sidecar sender would add a copy of every frame and a process, for nothing.
The two halves share the `srt` namespace on purpose.

## In four minutes

```sh
dev/harness/stage-plugins.sh          # build, and put the binary beside the manifest
gmx plugin add ./plugins/srt
gmx source add guest --type srt/source
```

That source now waits on UDP port 9000. Point an encoder at
`srt://<this machine>:9000` in caller mode and take it:

```sh
gmx take guest
```

To dial out instead of waiting:

```sh
gmx source add feed --type srt/source \
  --params '{"mode":"caller","host":"203.0.113.10","port":9000,"latency_ms":300}'
```

An address pasted from a hardware encoder goes in whole and wins over `host`
and `port`:

```json
{"uri": "srt://203.0.113.10:9000?mode=caller&latency=300&streamid=live/cam1"}
```

## Settings

| Key | Default | What it does |
|---|---|---|
| `mode` | `listener` | `listener` waits, `caller` dials out, `rendezvous` is both at once through a firewall |
| `host` | `0.0.0.0` | the interface to wait on, or the sender to dial |
| `port` | `9000` | the UDP port. SRT is UDP: a TCP firewall rule will not do |
| `uri` | empty | a whole `srt://` address; wins over `host` and `port`, and anything already in its query string is left alone |
| `latency_ms` | `125` | the delay budget, 0 to 10000 |
| `passphrase` | empty | 10 to 79 characters, the same at both ends. `format: secret`, so it is stored encrypted and never read back |
| `stream_id` | empty | the name the sender publishes under, where the far end sorts by it |
| `auto_reconnect` | `true` | dial again by itself when the link drops |

The full schema, with an example for every field, is `schemas/source.json`.

## What it costs

Three elements: `srtsrc ! queue ! fdsink fd=1`. The MPEG-TS that arrives is
handed to the core byte for byte. This plugin does not demux it, does not decode
it and never looks inside a buffer, so its own CPU is the kernel's copy and
nothing else. The core's container transport is `fdsrc ! decodebin`, so the
decode happens once, in the core, on the core's hardware aware path.

Declared latency is whatever `latency_ms` says, because that is the SRT receive
buffer and the aligner should know about it.

## Health

Once packets are arriving, `health` is `ok` with the link numbers in the detail:

```
rtt 14.2 ms, 31 lost, 4 retransmitted, 3.1 Mbit/s link
```

Before any packet arrives it is `degraded` with the reason. A `stats` call
returns the same numbers as JSON. The field names SRT publishes move between
GStreamer versions, so several spellings are tried and anything this build does
not publish is left out rather than reported as a zero.

## No picture

1. `health` says no packets have arrived. Nothing reached the port at all.
   Check the sender, then the firewall. SRT is UDP.
2. The passphrases differ. The handshake then fails with no packets and no
   error the receiving end can see. This is the usual cause.
3. `mode` must be the opposite of the far end's. Two listeners never meet.
4. A `stream_id` set here refuses a sender publishing under a different one.
   Clear it to accept any.

## Testing it

`cargo test -p gmx-srt` includes a test that starts a real sender on the
loopback with `gst-launch-1.0` and asserts that the stream arrives and that the
link reports itself. It says so and skips where `srtsrc`, `x264enc` or
`gst-launch-1.0` are missing.

The conformance harness needs something to receive, so start a sender first:

```sh
dev/harness/publish.sh srt 9000 &      # a test pattern, caller mode
gmx plugin test plugins/srt
```

Without a sender, checks 2 and 3 fail honestly: a source that receives nothing
produces no frames. `gmx plugin test plugins/srt --offline` needs no sender and
no core; it replays `tests/transcript.jsonl`.

## Where the rules come from

`docs/reference/plugin-manifest.md`, `docs/reference/plugin-protocol.md` and
`docs/reference/plugin-lifecycle.md`. The how to page is
`docs/how-to/srt.md`.
