# Use NDI

NDI is how a camera, a graphics machine and a mixer talk to each other on a
studio network without a capture card. A sender announces itself, a receiver
picks it by name, and nothing has an address to type.

This page takes about ten minutes, most of it installing the runtime.

## Install the NDI runtime first

GodwinMix does not ship it and cannot: the NDI runtime's licence does not permit
redistribution. Download it from
<https://ndi.video/for-developers/ndi-sdk/>. The runtime alone is enough; the
full SDK is not needed.

NDI is a registered trademark of Vizrt Group. GodwinMix is not affiliated with,
endorsed by, or sponsored by Vizrt.

The plugin loads the runtime at run time and never links against it, so it
builds and runs on a machine that has never seen NDI and tells you where to get
it rather than failing to load.

## Install the plugin and check both halves

```sh
dev/harness/stage-plugins.sh
gmx plugin add ./plugins/ndi
plugins/ndi/bin/gmx-ndi
```

Run by hand, the plugin prints what it found:

```
the NDI runtime is at /usr/local/lib/libndi.so.6
  STUDIO (CAM 1)  10.0.0.21:5961
  GRAPHICS (Titles)  10.0.0.44:5961
```

With no runtime it prints the download page instead. If an installer put it
somewhere unusual, set `NDI_RUNTIME_DIR_V6` to the directory holding the
library.

## Find the senders

```sh
curl -s -X POST localhost:8080/api/v1/tool/call \
  -H 'content-type: application/json' -d '{"name":"ndi/list_senders"}'
```

```json
{"senders": [{"id": "studio-cam-1", "name": "STUDIO (CAM 1)",
              "address": "10.0.0.21:5961", "width": 1920, "height": 1080, "fps": 30}]}
```

`id` is the slug a source would sensibly be added under. `name` is what goes in
the source's settings, exactly as reported, brackets and spaces and all.

`tool.call` is a registered method and the core hands it to a running instance
of the plugin, so an `ndi/source` has to be up before this answers. With no NDI
instance running the call says so and lists the tools that are there. Running
the plugin binary by hand, as above, prints the same list with no core at all.

## Add a source

```sh
curl -s -X POST localhost:8080/api/v1/sources \
  -H 'content-type: application/json' \
  -d '{"id":"cam1","uri":"ndi/source","type":"ndi/source",
       "params":{"name":"STUDIO (CAM 1)"}}'
gmx ctl take cam1
```

The sender's name is a setting, and `gmx ctl source add` carries an id, an
address, a `--type` and a name of its own but no flag for a plugin's settings,
so an NDI source is added over the API. `uri` is required there: the type id
goes in it when a source has no address of its own, which is what the command
line puts there too. A bare `ndi://STUDIO (CAM 1)` picks this plugin by its
scheme, but the plugin reads `name` and `address` and never the address it was
handed, so the sender would still be unset.

For a sender mDNS cannot reach, which means another subnet, a VPN, or a network
where multicast is filtered, give the address instead:

```sh
curl -s -X POST localhost:8080/api/v1/sources \
  -H 'content-type: application/json' \
  -d '{"id":"cam1","uri":"ndi/source","type":"ndi/source",
       "params":{"address":"10.0.0.21:5961"}}'
```

## Save bandwidth on a source nobody is looking at closely

```json
{"name": "STUDIO (CAM 2)", "bandwidth": "low"}
```

`low` is a much smaller picture at a fraction of the bandwidth. It is the right
answer for a source that is only ever in a corner of a multiview or a picture in
picture, and it is a real saving rather than a small one.

## Send the programme out as NDI

There is no command for this yet. `gmx ctl output add` takes an id and a URL,
and the core's output registry does not consult the plugin loader, so
`ndi/output` cannot be named from anywhere; see "What the core cannot do yet"
below. These are the settings it takes when it can be:

```json
{"name": "Studio A Programme"}
```

Anything running NDI on the network can then take the programme with no address
to type: another mixer, a graphics machine, a laptop at the back running OBS, a
monitor on the stage.

## What it costs

```
source   ndisrc ──► ndisrcdemux ──► matroskamux ──► the core
output   the core's programme ──► decode ──► ndisinkcombiner ──► ndisink
```

NDI's own codec is SpeedHQ and only the runtime can decode it, so `ndisrc` hands
over raw frames and raw frames are what cross to the core: one memcpy per frame,
no encode and no decode, on a pipe carrying 41 MB/s at 720p30 and 93 MB/s at
1080p30. Those are the numbers the media contract quotes for this transport.

The output pays a decode: the core hands it the programme already encoded and
`ndisink` wants raw. That is the protocol's price, not the plugin's. On a
machine already tight for CPU, an NDI output is the first thing to question.

## What the core cannot do yet

`ndi/source` works. Two things do not:

* **`ndi/output`.** The core's output registry
  (`crates/godwinmix-core/src/plugin/output.rs`) is a static list of the built in
  outputs and does not consult the plugin loader.
* **`ndi/discover`.** The loader interns only provides whose `kind` is
  `"source"` (`crates/godwinmix-core/src/plugin/loader.rs`), so a device provide
  is never started and nothing calls `discover`. `list_senders` itself is
  reachable over `tool.call`, because tools are declared once per plugin and any
  running instance answers for the whole plugin.

Both are complete against the contract and are exercised offline:

```sh
gmx plugin test plugins/ndi --offline --provide discover
```

which runs with no runtime and no core.

## When a sender cannot be found

1. Both machines must be on the same subnet. NDI finds senders with mDNS and
   mDNS does not cross a router. Give `address` instead.
2. A listen shorter than about half a second finds nothing on a quiet network
   even when senders are there. The announcements take about a second.
3. Some networks filter multicast. Managed switches and guest wifi both do.
4. Check the runtime is actually found, by running the plugin binary by hand.

## Lip sync across several NDI cameras

Leave `timestamp_mode` alone unless sync is wrong. When it is, try `timecode`,
which uses the sender's own timecode rather than the time the frame arrived:

```json
{"name": "STUDIO (CAM 1)", "timestamp_mode": "timecode"}
```

## Where to go next

* [The network plugins, every setting](../reference/plugins-network.md)
* [Receive a phone or an OBS stream](receive-a-phone-or-obs-stream.md)
* [Install a plugin](install-a-plugin.md)
