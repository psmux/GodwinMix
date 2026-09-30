# AGENTS.md

For a coding agent changing this plugin. Read this before editing anything.

## What this is

`udp/source` and `udp/output`: MPEG-TS over UDP and RTP, unicast and
multicast. One binary; `GMX_PROVIDE` says which provide a process serves.
Built on `godwinmix-sdk`, `plugins/netkit` for the pipeline and bus watch, and
`plugins/capture-common` for the programme FIFO an output reads.

The source is a byte mover with a filter in it:
`udpsrc ! (probe) ! queue leaky ! fdsink fd=1`. The probe reads four header
bytes a packet, drops stuffing and unwanted programs, and counts loss. The
core demuxes and decodes. Do not add a demuxer or a decoder to the source.

The output remuxes: `appsrc ! matroskademux ! parsers ! mpegtsmux ! udpsink`.
Do not add an encoder: the programme's encode is the only one.

## Build and test

```sh
cargo test -p gmx-udp                      # unit tests plus real sockets on this machine
cargo clippy -p gmx-udp --all-targets      # add no warning
./build                                    # stage bin/gmx-udp
gmx plugin test plugins/udp --offline      # replay tests/transcript.jsonl
python3 tests/drive.py --uri udp://@239.1.1.1:5000 --seconds 60   # a soak, with counters
```

On macOS, replace an installed `bin/gmx-udp` by removing it first and copying
the new one in. Copying over the file in place leaves a binary the kernel
kills on launch for an invalid code signature, and the core then reports that
the plugin never sent `initialize`.

## Where things are

| Path | What it is |
|---|---|
| `src/main.rs` | picks the provide |
| `src/source.rs`, `src/output.rs` | the trait implementations and their errors |
| `src/address.rs` | `udp://` and `rtp://` addresses, VLC and ffmpeg forms |
| `src/iface.rs` | the interface multicast uses; the one platform specific file |
| `src/counters.rs` | atomics the streaming thread adds to and the health thread reads |
| `src/recv/` | the receive pipeline, the probe, health, settings |
| `src/send/` | the send pipeline, one branch per programme stream, settings |
| `src/ts/` | TS headers, CRC, section reassembly, PAT, PMT and SDT, the plan of what to keep, the filter, RTP |
| `schemas/` | the settings of each provide, JSON Schema 2020-12 |
| `skills/` | what an agent operating the mixer needs |
| `tests/transcript.jsonl` | a recorded conversation, replayed offline |
| `tests/drive.py`, `tests/lossy_send.py` | a stand in core and a lossy sender, for measuring by hand |

## The rules that matter

1. **stdout is media** in `udp/source`. Log through the `Reporter`.
2. **The probe never waits.** It runs on `udpsrc`'s streaming thread for every
   datagram. It takes no lock another thread holds for long (the catalog is
   published with `try_lock`), allocates only for tables, and parses a table
   only when its CRC changes.
3. **Loss is counted, never waited for.** No jitter buffer, no reordering. A
   late RTP packet is not loss and not a reason to hold anything.
4. **No port until `start`.** `initialize` validates and opens nothing; `stop`
   and a dropped `Receiver` close the socket.
5. **The output opens its FIFO at `initialize`.** The core opens its end
   before calling `start`, and both opens wait for the other.
6. **Multicast out of an interface is `IP_MULTICAST_IF`.** `udpsink`'s
   `multicast-iface` only joins. See `src/iface.rs`.
7. **Every error names the next step.**

## What is deliberately not here

* No FEC (SMPTE 2022-1). It would mean listening on two more ports.
* No `keyframe-request`. UDP has no back channel.
* No `seek`. A live feed has no timeline.
