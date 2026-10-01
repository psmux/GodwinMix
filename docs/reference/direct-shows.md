# Direct shows: the host's table, events and calls

A show with compositing off is one input straight to its outputs, run by the
direct host inside the ingest plugin's channel server (`ingest/discover`),
not by a show process of its own. This page is the interface between the
station and that host. The station's side (the show registry, `show.*`, the
wall) is in `docs/reference/shows.md`; why the host is built the way it is,
in `docs/explanation/direct-host.md`.

## The table

The station lays the table over the plugin's settings as `direct` and calls
`configure`. The whole table comes every time; the host compares it with
what it runs and changes only what changed.

```json
{"direct": [
  {
    "id": "bbc-one",
    "name": "BBC One",
    "input": {
      "uri": "udp://@239.1.1.1:5000",
      "program": 101,
      "params": {"interface": "en0"},
      "backup": {"uri": "srt://10.0.0.2:9000?mode=caller"}
    },
    "outputs": [
      {"id": "copy", "platform": "udp", "url": "udp://239.2.2.2:5000?ttl=4&interface=en0", "stream": "main"},
      {"id": "yt", "platform": "youtube", "url": "rtmp://a.rtmp.youtube.com/live2/KEY", "stream": "main",
       "rendition": true, "video": "encode:main:h264:1280x720p30:3000k:g60", "audio": "copy:main:audio"}
    ],
    "transcode": [{"stream": "main", "nodes": ["..."]}],
    "monitor": {"alarms": true, "pictures": false, "thresholds": {"black_secs": 5}}
  }
]}
```

| Field | Meaning |
|---|---|
| `id` | the show, a slug. The input is published on the hub as `direct.<id>/main` |
| `input` | what to open; the inputs and their `params` are the input work's, in `plugins/ingest/src/direct/input/` |
| `outputs` | each exactly a channel destination row; only outputs that are on are listed |
| `transcode` | exactly a channel row's `transcode`, planned and admitted by the station |
| `monitor` | handed to the vitals as it is |

What a change does:

| Change | What the host does |
|---|---|
| a new row | publishes `direct.<id>/main`, opens the input, starts the outputs |
| a row gone | stops the input and ends the stream for every reader at once |
| `input` changed | stops the old input and opens the new one; the outputs start again |
| an output added, changed or removed | starts, restarts or stops that output; the others keep sending |
| `transcode` changed | rebuilds only the nodes whose description changed |

A row that cannot be read (no `id`, an `input` with no address, an id twice)
is left out and logged with the reason.

## Outputs

Every output is one thread reading the show's stream from the hub. Where it
goes is the scheme of `url`; `platform` agrees with it.

| `url` | What is sent | Options in the query |
|---|---|---|
| `rtmp://`, `rtmps://` | the tags as they are, no decode | none |
| `srt://` | MPEG-TS through GStreamer's `mpegtsmux` and `srtsink` | as `srtsink` takes them |
| `udp://host:port` | MPEG-TS, seven packets to a datagram, unicast or multicast | `ttl` (default 16), `interface` (a name or an address) |
| `rtp://host:port` | the same in RTP, payload type 33, one sequence number per datagram | as `udp://` |
| `rist://host:port` | MPEG-TS in RTP through `ristsink`, with retransmission; the port must be even | `buffer` in ms (default 1000) |
| `file:///path/name.ts` | MPEG-TS written to the file; a name already there gets the time added | none |

UDP, RTP, RIST and files are muxed by the plugin's own MPEG-TS muxer
(`plugins/ingest/src/tsmux/`), H.264, HEVC and AAC, nothing decoded. A
channel destination takes the same addresses.

An output with `rendition = true` reads the pair named by `video` and
`audio` instead of the input, from the conversion the `transcode` nodes
build. The input is decoded once per show however many renditions it has.
With `rendition = true` and no `video` yet the output waits and sends
nothing.

## Events

All four are under `direct.`, which the station routes to its show registry.

`direct.input`, when the input goes live or idle, moves to or from its
backup, changes shape, or cannot be opened:

```json
{"show": "bbc-one", "state": "live", "since_ms": 1790844395731, "from": "239.1.1.1:5000",
 "backup": false, "relay": "127.0.0.1:1935", "stream": "direct.bbc-one/main",
 "video": {"codec": "h264", "width": 1280, "height": 720, "fps": 29.97, "kbps": 2600},
 "audio": {"codec": "aac", "channels": 2, "sample_rate": 48000, "kbps": 128}}
```

`idle` means no frame for three seconds. An input that would not open is
`idle` with `error` saying why. `relay` and `stream` are what an
`ingest/rtmp` source is given to read the input, which is how a show with
compositing on takes it.

`direct.output`, when an output's state, error or reconnect count moves:

```json
{"show": "bbc-one", "output": "copy", "state": "live", "since_ms": 1005, "kbps": 2850, "reconnects": 0, "error": null}
```

`direct.stats`, once a second while any show runs, every show in one event:

```json
{"shows": [{"id": "bbc-one",
  "input": {"state": "live", "kbps": 2729, "fps": 29.97, "width": 1280, "height": 720,
            "video_codec": "h264", "audio_codec": "aac", "audio_channels": 2,
            "cc_errors": 0, "packets_lost": 0, "keyframe_ms": 1000, "last_frame_ms": 12},
  "outputs": [{"id": "copy", "state": "live", "kbps": 2850, "reconnects": 0}],
  "dropped_gops": 0}]}
```

The input's own numbers come first; what it leaves blank is filled from the
hub's meter, and what nobody can tell yet is left out. `encoder` is on an
output that converts. `dropped_gops` counts the GOPs readers of the show
lost by falling behind. `cpu_millicores` per output is not measured.

`direct.health` is raised by the vitals, on a change of state or of the
set of alarm kinds.

## Calls

The station makes these as `tool.call` on `ingest/discover`:

| Call | Answer |
|---|---|
| `direct.stats {ids?}` | `{shows: [...]}` as in the event, every show or those in `ids` |
| `direct.thumbnail {show, width?}` | the vitals' newest picture, `{jpeg, width, height, at_ms}` with the JPEG in base64, or `{pending: true}` |

Any other `direct.` name is refused with `-32601` and the two it answers.

## Running the host alone

```sh
cargo build --release -p gmx-ingest
target/release/gmx-ingest --direct table.json --seconds 60
```

`table.json` is the table above, or the bare array of rows. The file is read
again whenever it changes, so shows can be added and removed while it runs.
Every five seconds one line goes to stderr:

```text
t=10s shows=50 inputs_live=50 outputs_live=50/50 in_kbps=117158 out_kbps=119740 dropped_gops=0 cc_errors=0 cpu=62.6% rss_mb=33.5
```

`cpu` is the process's share of one core since the line before. The input
and output events are printed as they happen. No station, no channel and no
listening port are needed; a `channel:` input has no channels to read here.

## What is not here yet

* HLS from a direct show. The packager is the station's (`hls/output`);
  every show's stream and every rendition pair is on the hub the relay
  serves, so the station can read one with `GMXHUB direct.<id>/<name>` and
  package it with nothing decoded again. That station side is not written.
* Per output CPU, which needs a per thread clock this host does not read.
