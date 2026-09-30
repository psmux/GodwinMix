# The udp plugin

`udp/source` and `udp/output`: MPEG-TS over UDP, bare or inside RTP, unicast
or multicast. The code is [`plugins/udp`](../../plugins/udp/README.md). How to
use it from the page is [Receive and send MPEG-TS over UDP and
multicast](../how-to/udp-and-multicast.md).

## Addresses

Both provides read the forms VLC and ffmpeg write. Anything after `?` is
ignored, so an address copied from an ffmpeg command line works as it is.

| Written | Means |
|---|---|
| `udp://@239.1.1.1:5000` | multicast group 239.1.1.1, port 5000 |
| `udp://239.1.1.1:5000` | the same; the `@` is optional |
| `udp://10.0.0.9@232.1.1.1:5000` | source specific multicast: the group, only from 10.0.0.9 (receive only) |
| `udp://0.0.0.0:5000`, `udp://@:5000` | unicast, every interface (receive) |
| `udp://192.168.1.50:5000` | unicast to one receiver (send) |
| `rtp://...` | any of the above, with RTP around the TS |
| `udp://[ff05::1]:5000` | IPv6, in brackets |

## `udp/source`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `address` | string | `0.0.0.0` | a multicast group to join, or the local address to receive unicast on |
| `port` | integer 1 to 65535 | `5000` | the UDP port |
| `uri` | string | empty | a whole address from the table above; wins over `address` and `port` |
| `program` | integer 0 to 65535 | `0` | the program number to take out of a multiplex. 0 is the first the PAT lists |
| `interface` | string | empty | the interface to join multicast on, by name or by one of its IPv4 addresses. Empty is the default route's (Advanced) |
| `source_address` | string | empty | accept the group only from this sender (IGMPv3). Refused on a unicast address (Advanced) |
| `pids` | string or array | empty | elementary stream PIDs to keep, `"256, 0x101"`. The PMT is rewritten to list only these (Advanced) |
| `receive_buffer_kb` | integer 64 to 262144 | `4096` | the socket's kernel receive buffer. The system may cap it (Advanced) |

`uri_schemes`: `udp://` and `rtp://` at rank 240, above `hls/source`'s 200,
which claims both through `uridecodebin` and can neither choose a program nor
count loss.

`capabilities`: `restart-in-place`, `health`, `latency-report`. Latency is
reported as 0: there is no jitter buffer and nothing is retransmitted.

Media: `container`. The chosen program crosses to the core as MPEG-TS, and the
core demuxes and decodes it once, as it does for `srt/source`.

### What happens to each datagram

On the streaming thread, per datagram:

1. The first byte says what it is: `0x47` is bare TS, `0x80` to `0xBF` is RTP
   version 2. RTP's header, CSRCs, extension and padding are stripped, and a
   gap in its sequence numbers is counted as `rtp_packets_lost`. Anything else
   is counted as `malformed` and dropped.
2. Null packets (PID 0x1FFF) are dropped and counted.
3. A gap in a PID's continuity counter is counted as `ts_packets_lost`. A
   packet with the discontinuity flag set is not a gap.
4. The PAT, every PMT and the SDT are reassembled and parsed, only when their
   CRC changes.
5. With one program and nothing chosen, every other packet passes untouched,
   tables and all, and the original buffer is handed on without a copy. With
   several programs, or a program or PIDs chosen, a PAT naming only the chosen
   program is written in place of the original, the chosen PMT is passed (or
   rewritten when `pids` leaves streams out), and only its PCR and elementary
   PIDs pass.

Before the first PAT nothing passes; the core would discard it anyway. A
datagram after more than a second of silence resets the counters, so a sender
that restarted is not counted as loss.

A queue of one second, leaky at its old end, sits between the socket and the
core. A core that is busy for a moment costs the oldest datagrams, never a
blocked socket.

### Health

| State | When | Detail, for example |
|---|---|---|
| `degraded` | not started | `not started yet, so no port is open` |
| `degraded` | started, nothing received | `listening on udp://@239.1.1.1:5000 and nothing has arrived yet. Check that the sender ...` |
| `degraded` | silent for over 2 s | `the feed on udp://@239.1.1.1:5000 stopped 4 s ago. Still listening; ...` |
| `degraded` | the chosen program or PIDs are not in the feed | `program 9 is not in this feed. It carries 1 (News) [h264 PID 256, aac PID 257]; ...` |
| `degraded` | more than 0.5% of packets lost in the last second | `losing 1.2% of packets (9 in the last second). The picture carries on with damage. ...` |
| `ok` | flowing | `8.3 Mbit/s on udp://@239.1.1.1:5000, 0 packets lost; program 2 (Sport) of 3: 1 (News), 2 (Sport), 3 (Film)` |
| `failing` | the port or the group could not be opened | `udp://0.0.0.0:5000 is already taken on this machine: another source or program is receiving on that port. Choose another port, or remove the other source.` |

### Calls

`stats`:

```json
{"address": "udp://@239.1.1.1:19471",
 "stats": {"datagrams": 48417, "bytes_in": 62544968, "bytes_out": 62544968,
           "null_packets_dropped": 0, "ts_packets_lost": 0, "rtp_packets_lost": 0,
           "malformed": 0, "flagged_by_sender": 0, "resumed": 0, "silent_ms": 12}}
```

`programs`:

```json
{"chosen": 1, "problem": null,
 "programs": [{"program": 1, "name": "", "provider": "", "pmt_pid": 4096, "pcr_pid": 256,
               "streams": [{"pid": 256, "kind": "h264", "language": null},
                           {"pid": 257, "kind": "aac", "language": null}]}]}
```

`kind` is one of `h264`, `hevc`, `mpeg2 video`, `aac`, `aac latm`,
`mpeg audio`, `ac3`, `eac3`, `teletext`, `subtitles` or `data`. `name` and
`provider` come from the SDT's service descriptor, when the feed carries one.

The core does not yet put a plugin's `call` answers or its health detail on
the page; `stats` and `programs` are reachable by a client that talks to the
plugin, and `tests/drive.py` in the plugin prints both.

`configure` with different settings on a running source answers
`restart_required`: a socket binds and joins once.

## `udp/output`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `uri` | string | required | where to send, from the table above. A source address or port 0 is refused |
| `ttl` | integer 1 to 255 | `8` | multicast TTL, and the unicast TTL too (Advanced) |
| `interface` | string | empty | the interface multicast leaves by, by name or by one of its IPv4 addresses (Advanced) |
| `cbr_kbps` | integer 0 to 1000000 | `0` | 0 sends what the programme makes. Anything else pads with null packets to that rate and caps the sending a hair above it (Advanced) |
| `packets_per_datagram` | integer 1 to 7 | `7` | TS packets per datagram; 7 is 1316 bytes (Advanced) |
| `dscp` | integer -1 to 63 | `-1` | the DiffServ code point; -1 leaves the system default (Advanced) |

`uri_schemes`: `udp://` and `rtp://` at rank 240. `capabilities`:
`restart-in-place`, `health`.

The pipeline, after the core's FIFO:

```text
appsrc ─► matroskademux ─┬─► queue ─► h264parse ─┐
                         └─► queue ─► aacparse  ─┴─► mpegtsmux ─► [rtpmp2tpay] ─► udpsink
```

H.264, HEVC, MPEG-2 video, AAC, MPEG audio, AC-3 and Opus are carried. A
stream MPEG-TS has no mapping for is left out and the log says so. The H.264
and HEVC parameter sets are repeated before every keyframe, so a receiver that
tunes in late starts at the next one. With `rtp://`, the payloader uses
payload type 33 and one datagram per seven packets.

Health is `ok` with the rate while bytes leave, `degraded` when nothing has
left in the last second, and `failing` with the reason when the pipeline
fails. UDP has no answer from the far end, so `ok` means sent, not received.

`stats`: `{"address": "udp://@239.1.1.1:5000, TTL 8", "bytes_sent": 1234567}`.

## Platforms

| | macOS (tested here) | Linux | Windows |
|---|---|---|---|
| `udp/source`, unicast | yes | not tested | not tested |
| `udp/source`, multicast, default interface | yes, and two receivers of one group | not tested | not tested |
| `udp/source`, `interface` by name | yes, joined on `lo0`; an unknown name is refused | not tested; GLib turns an unknown name into the default route | not tested |
| `udp/source`, source specific | not tested | not tested | not tested |
| `udp/output`, unicast and multicast | yes | not tested | refused by the core: a sidecar output gets the programme on a FIFO, and Windows has none |
| `udp/output`, `interface` | yes, out of `lo0` | not tested | not applicable |

The interface is the one place the plugin differs by platform, and it is
`cfg` gated in `plugins/udp/src/iface.rs`:

* Receiving uses `udpsrc`'s `multicast-iface`, which GLib turns into the join
  on that interface. An IPv4 address is turned into the interface's name
  first, because `udpsrc` wants a name.
* Sending cannot use `udpsink`'s `multicast-iface`: in GStreamer 1.28.7 it is
  used to join a group and never to choose where packets leave, so with it set
  to `lo0` nothing left by the loopback. On Unix the plugin sets
  `IP_MULTICAST_IF` itself, on the socket `udpsink` opened, with the
  interface's IPv4 address from `getifaddrs`, before the first packet. On
  Windows there is no FIFO for an output, so the question does not arise yet;
  when it does, the same option is `setsockopt` on a `SOCKET` and needs the
  Windows socket API.

Constant bitrate: at 8000 kbit/s the output measured 8.03 Mbit/s over ten
seconds, 1316 byte datagrams, 19.9% null packets. The rate is capped, not
clocked: in 100 ms windows it ran from 4.7 to 11 Mbit/s, because the
programme arrives a frame at a time. A modulator with a buffer takes that; one
that needs packets spaced to the microsecond is not served yet.
