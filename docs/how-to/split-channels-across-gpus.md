# Split channels across GPUs

A machine with two NVIDIA cards can carry twice the channels of one, but only
if the channels are shared out. This page is about putting each channel's
conversion on a card: by number, or by load with `auto`, and evening the
cards out again later.

Everything a channel converts on the GPU runs on one card: the decode on that
card's NVDEC, the scale and pad in its memory, and the encode on its NVENC.
A picture never crosses from one card to the other, and the process keeps one
CUDA context per card, shared by every channel on it.

## Spread a headend's channels

For a headend that follows a Flussonic file, nothing needs setting: an
imported `transcoder deviceid=N` is read as `auto`, so every channel goes on
the card with the least work when it starts. Flussonic writes `deviceid=0` on
every stream unless told otherwise, so taking the file at its word would put
every channel on the first card. To say it in the config anyway:

```toml
[headend]
flussonic = "/etc/flussonic/flussonic.conf"
# spread (the default): every channel on the card with the least work.
# file: deviceid=N puts a channel on card N, as the file says.
flussonic_deviceid = "spread"
```

The same choice is in the import dialog (**Import** on the channel list) on a
machine with more than one card, with spreading first, and in
`headend.import` as `deviceid`.

For the channels set up on the headend screen, the editor's **GPU** field is
**Auto** unless a card is picked, and the channel list has **Spread across
GPUs** and **Move to GPU** for the ticked channels.

## Put one channel on a card

In a rendition, `video.gpu` is a CUDA device index, or `auto`:

```json
{"video": {"codec": "h264", "width": 1280, "height": 720, "bitrate_kbps": 1500, "gpu": 1},
 "audio": {"codec": "aac", "bitrate_kbps": 128}}
```

A headend channel takes it as `gpu` (`headend.channel.update`, or
`headend.channel.bulk {names, gpu: 1}` for many). A card the machine does not
have is placed as `auto`, so a file written for another machine never stops a
channel. `gpu.status` numbers the cards the same way.

A rendition that names no card stays on card 0, as before there were several,
unless the station says otherwise:

```toml
[hardware]
gpus = "spread"   # first (the default) or spread
```

## Why a channel does not move on its own

`auto` places a channel once, when it is first planned, and it stays on that
card for as long as it runs. Moving it means restarting its encode, and its
HLS output starts a new segment, so a player behind it skips one. A card
getting busier does not move anything.

When the cards drift apart, even them out on purpose:

* On the headend screen, **Even out the cards** in the GPU strip. The first
  press says how many channels would move; a second press moves them.
* Or call `gpu.rebalance {dry_run: true}` to see the moves, then
  `gpu.rebalance {}`.

It moves the fewest channels placed by `auto` from the busiest card to the
idlest, by the work the governor holds on each. A channel set on a card by
number never moves.

## Check it

The GPU strip on the headend screen shows each card's encoder, decoder and
memory, and the channels converting on it. From the API, `gpu.status` answers
`channels` for each card, `headend.channel.list` answers each channel's
`on_gpu`, and `governor.status` lists the work held on `nvidia` (card 0) and
`nvidia:1` (card 1) apart.

On the machine itself:

```sh
nvidia-smi --query-gpu=index,utilization.encoder,utilization.decoder,memory.used --format=csv
nvidia-smi pmon -c 1
```

`pmon` lists the process once per card it encodes and decodes on, with its
`enc` and `dec` share on each.

## What it needs

GStreamer's NVIDIA plugin registers its decoders and encoders once per card:
`nvh264dec`, `nvmpeg2videodec` and `nvh264enc` on the first,
`nvh264device1dec`, `nvmpeg2videodevice1dec` and `nvh264device1enc` on the
second (`gst-inspect-1.0 nvcodec` lists them). A card is offered only when it
has every NVIDIA decoder and encoder the first one has. Inside a container,
`--device nvidia.com/gpu=all` hands over every card
([Convert broadcast feeds on the GPU](convert-broadcast-feeds-on-the-gpu.md)).

CUDA numbers the cards fastest first and NVML (`nvidia-smi`, `gpu.status`) by
their place on the bus. On a machine of identical cards, or one whose fastest
card is also first on the bus, the two agree in practice. Where they might
not, set `CUDA_DEVICE_ORDER=PCI_BUS_ID` in the station's environment so they
do.
