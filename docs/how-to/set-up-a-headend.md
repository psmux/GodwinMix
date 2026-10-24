# Set up a headend in GodwinMix

This page is for someone starting a headend from nothing: a machine that
takes TV programmes off multicast (or SRT, RTMP, RTSP, HLS), converts the
ones that need it, and serves every one as HLS for players, set top boxes
and a CDN. You do not need Flussonic or any other headend software, and you
do not need a config file. Everything is set up on the headend screen, or
through the same methods by a script or an agent.

If you already run Flussonic and want to move its config over, read
[Replace Flussonic with GodwinMix](replace-flussonic.md) instead. An import
makes the same channels this page makes by hand, and you can edit them here
afterwards.

## What a channel is

A channel is one programme in and one HLS stream out. In GodwinMix it runs
as a direct show: a show with compositing off, so there is no mixer, no
scene and no programme encode. The input goes straight to the outputs, and
it is decoded only when you ask for another format.

Each channel has:

* a **name**, which is its public path. A player opens
  `/<name>/index.m3u8`, so pick something that will not change, such as
  `CH101` or `news-hd`;
* an **input**, and if you want one, a **backup input** that takes over when
  the main one stops;
* a **profile**: pass through as it comes, or a conversion such as 720p
  H.264 at 1.5 Mbit/s;
* its **HLS output**, which every channel has, and any further outputs: a UDP
  multicast out, an SRT or RTMP push.

The station keeps every channel in `headend.json` beside its list of shows,
so they come back after a restart.

## Start the station

Any GodwinMix station can keep headend channels. To run a machine that does
nothing else, start it in headend mode, which turns mixing off:

```sh
godwinmix headend --port 8090
```

`--port` is optional. With it, the station also answers on that port the
URLs a Flussonic server would, `/<name>/index.m3u8` and
`/streamer/api/v3/streams`, so a gateway, a cache or a monitoring page
written for Flussonic can read it. Leave it out if nothing in front of the
machine needs those paths; the station's own HLS links work either way.

Open the station's page in a browser. A headend station opens on the
headend view. On any other station, open View > Headend.

## The first channel

With no channels yet, the headend screen offers three ways in:

* **Add a channel**: one channel, by hand.
* **Scan a multicast feed**: list what a multiplex carries and add the
  programmes you tick.
* **Import a Flussonic config**: paste one or point at the file.

Press **Add a channel** and fill in:

1. The name, and a title for the screen if you want one.
2. The input. Pick its kind, then give its address:

   | The feed | What to give |
   |---|---|
   | A multicast group (an IRD, an encoder, a satellite receiver) | the group and port, `239.0.0.1` and `5000`, and the network card it arrives on |
   | MPEG-TS sent to this machine | the port |
   | RTP wrapped TS | the group and port; RTP is read from the packets |
   | An SRT encoder that listens | `srt://192.0.2.10:9000`, and its passphrase |
   | An SRT encoder that calls you | the port to listen on |
   | Someone else's RTMP server | `rtmp://host/app/streamkey` |
   | An IP camera | `rtsp://192.0.2.20/stream1` |
   | An HLS stream to pull | `https://example.com/live/index.m3u8` |
   | A file to loop | `file:///clips/loop.ts` |

   A multiplex carries several programmes: give the **program number** of
   the one you want. Left empty, the first is taken.
3. The profile. **Pass through** sends the programme as it comes, which
   costs almost nothing. Pick a conversion when the players need another
   size, codec or rate, or when the sound is AC-3 or MPEG layer II, which
   HLS players do not all play: **Picture as it comes, sound to AAC** fixes
   only the sound.
4. Outputs. HLS is always there. Under its settings, segments are MPEG-TS
   and CMAF both, and access is open to the local network by default, for
   a gateway or cache in front of the machine. Add a UDP, SRT or RTMP
   output if something else takes the channel too.

Save. The channel appears as a tile and goes live within a few seconds. Its
HLS link is in the channel's drawer.

## Add a whole multiplex at once

Press **Scan a multicast feed**, give the group, the port and the card, and
press Scan. The station listens for about eight seconds and lists every
programme: its number, its name and provider from the SDT, its picture size
and frame rate, and each sound track with its codec, channels and language.
A programme marked scrambled has no key on this machine and will show no
picture. A programme another channel already takes says which.

Tick the programmes you want, pick one profile for all of them, and add
them. Each becomes a channel named from its SDT name, or from its number
when the feed sends no names. Rename any of them afterwards.

Every channel taken from one group shares one socket: the group is joined
once and its tables are read once, however many programmes you take.

## Profiles

A profile is one conversion with a name, and many channels can take it.
Change the profile and every channel on it follows, which is how a whole
headend moves to another bitrate in one step.

The built in profiles:

| Profile | What it makes |
|---|---|
| Pass through | the programme as it comes |
| Picture as it comes, sound to AAC | the picture copied, every sound track as AAC 128 kbit/s |
| 480p H.264, 800 kbit/s | 854x480 |
| 720p H.264, 1.5 Mbit/s | 1280x720 |
| 1080p H.264, 4 Mbit/s | 1920x1080 |

Each picture profile fits the picture into its size with black bars,
deinterlaces what the stream marks as interlaced, puts a keyframe every two
seconds and converts every sound track to AAC at 128 kbit/s.

Save your own from the profiles list: codec, size and how the picture fits
(bars, crop or stretch), bitrate, keyframe interval in seconds or frames,
deinterlacing, sound codec, bitrate and whether to convert the first sound
track or all of them. The encoder is Auto by default: a GPU encoder first
(NVENC, Quick Sync, VideoToolbox) and the CPU when the GPU is full or has
no encoder for that codec. Pick GPU or CPU to keep a profile on one of
them; GPU is offered only on a machine that has one. A profile some
channel takes cannot be removed until you say which profile those
channels move to.

A channel can also carry a conversion of its own, without a profile: pick
Custom in its editor.

## Change many channels at once

In the channel list, tick the channels and pick an action: give them a
profile, enable or disable them, move their multicast inputs to another
network card, or remove them. Only a change of input, conversion or
outputs restarts a channel; a title or enabling one moves it in place.

## When a Flussonic config is followed as well

A station started with `godwinmix headend --flussonic FILE` follows that
file, and its streams show on the headend screen beside the channels kept
here, marked with the file they come from.

Editing one of those streams on the screen takes it out of the file's
hands. The station keeps the channel from then on, its show is handed over
without a restart, and later edits to that stream in the file are not
applied. The screen says so before you save, and the channel is marked as
taken over. **Follow the file again** gives it back: the station forgets
its own copy and runs the stream as the file says. Removing a followed
stream on the screen hides it from the file the same way; the file itself
is never edited.

## From a script or an agent

Everything on the screen is a station method, so a script does the same:

```sh
gmx call headend.scan '{"uri": "udp://@239.0.0.1:5000", "interface": "eth1"}'
gmx call headend.channel.add '{"channels": [
  {"name": "CH101", "input": {"uri": "udp://@239.0.0.1:5000", "program": 1, "interface": "eth1"}, "profile": "h264-720p-1500"},
  {"name": "CH102", "input": {"uri": "udp://@239.0.0.1:5000", "program": 2, "interface": "eth1"}, "profile": "copy"}
]}'
gmx call headend.channel.bulk '{"names": ["CH101", "CH102"], "profile": "h264-480p-800"}'
```

`"dry_run": true` on an add checks and prices the channels and makes
nothing. `headend.export` writes the channels kept here in Flussonic's
config language. [The reference](../reference/headend-channels.md) has
every method and field.

## Related

* [Watch a headend of many channels](watch-a-headend.md): reading the tiles, the GPU strip and the input groups.
* [Take headend feeds into direct shows](headend-feeds.md): every kind of input address, programs, backups and the numbers an input reports.
* [Convert broadcast feeds on the GPU](convert-broadcast-feeds-on-the-gpu.md).
* [Headend mode](../explanation/headend-mode.md): why it is built this way.
