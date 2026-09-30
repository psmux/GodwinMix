# Send a channel on to YouTube, Facebook or Twitch

A channel is a place an encoder publishes to on the mixer (see
[take streams from several encoders](channels.md)). Its stream can be
passed straight on to one platform or several at once, as it arrives, without
going through the mixer's programme. Nothing is decoded or encoded on the way,
so each destination costs a copy of the bytes and not much else.

This is how a service goes to YouTube and Facebook at once from the one
encoder in the rack, while the mixer's own programme goes somewhere else or
nowhere.

## Add a destination

Every channel card on the Channels tab ends in a strip headed Send on to. On a
channel with no destinations yet it offers YouTube, Facebook, Twitch and Kick,
and **More** for the rest: Instagram Live, LinkedIn Live, X, TikTok LIVE,
Custom RTMP and SRT. Once there is one, the strip shows the destinations as
tiles and an **Add** tile opens the same choice.

Press a platform, paste its stream key, press **Start sending**.

| Platform | Where to copy the key from |
|---|---|
| YouTube | YouTube Studio, Go live, Stream settings. The stream key, not the stream URL |
| Facebook | The Live producer page, Streaming software |
| Twitch | The Creator Dashboard, Settings, Stream. The primary stream key |

For YouTube, Facebook and Twitch the server is filled in and not even shown.
Kick and X start with their published server in the box, which you can paste
over if your dashboard gives a different one. Instagram, LinkedIn and TikTok
hand out a new server address with every stream, so their form asks for both
halves and says where on the platform to find them.

**Custom RTMP** takes a server and a key, or a whole address in the Server box
with the key box left empty (`rtmp://10.0.0.9:1935/live/hall`). **SRT** takes
an `srt://` address and no key.

**More options** names the tile and, on a channel with several streams, picks
which one to send. Left alone it sends the stream that has been live longest,
and if that one stops while another is still live, it moves on to that one.

The key is never shown again after it is saved.

## Read the tiles

Each destination is a tile with the platform's mark in a ring, and a switch.

| The tile says | The ring | What it means |
|---|---|---|
| Off | grey | switched off with its switch |
| Waits for the stream | dashed | on, and nothing is being published to the channel yet |
| Connecting | amber, turning | dialling the platform |
| Live, 2.6 Mb/s | green | sending, at that rate |
| Trying again | amber, turning slowly | the platform went away and is being dialled again |
| Stopped | red | the platform refused the key three times and it has stopped asking |
| Needs a key | red | a platform that needs a key has none |

A tile that is trying again or has stopped says why under its state: "nothing
answered at rtmp://10.0.0.9:1935" means the server is down or the address is
wrong; "YouTube refused the key" means paste it again from the platform.

When one platform drops, the others carry on and nothing reaches the encoder.
The one that dropped is dialled again: gently for YouTube, Facebook and
Twitch, which penalise a client that hammers them, and quickly for a server of
your own. When it comes back it starts at the next keyframe, so viewers see
the picture return rather than a smear. A destination that falls behind loses
whole seconds of picture rather than slowing anything else down.

## Send one destination a smaller picture

Some destinations do not want the stream as the encoder sends it: a 1080p
encoder and a church hall projector that only takes 720p, or a phone
network that cannot carry 6 Mb/s. A destination can ask for a format of its
own, and only that destination is converted. The page for this arrives with
the renditions work; the steps below are what it shows.

In the destination's form, under the stream key, **Format** starts on
**Same as the source**, marked Free. Pick a preset instead, **YouTube 720p30**
for instance, or **Custom** for a size, frame rate and bit rate of your own,
then **Start sending** or **Save**.

The tile then says what was done under its state:

| The tile says | What it means |
|---|---|
| Copied | the stream already is what was asked for, so it goes out untouched, at no cost |
| GPU encode | converted on the graphics chip (VideoToolbox on a Mac) |
| CPU encode | converted in software, because this machine has no hardware encoder or it is full |
| …, shared | another destination asked for the same format, and both get the one encoder's output |

The stream is decoded once however many destinations convert it, and every
destination that asks for the same format shares one encoder, so three
destinations at 720p cost about what one does. A destination left on Same as
the source still costs only a copy of the bytes.

If the machine has no room for the conversion, nothing is started and the
form says so, with what it would cost, what is free, and a button for each
format that would fit. Press one to use it. When the machine runs short
while live, say because another program took the CPU, a converting
destination is stopped before anything the mixer has on air, its tile says
why, and it starts again by itself once there has been room for half a
minute.

When the encoder changes size in the middle of a stream, the conversion
follows: only the part that has to change is rebuilt, and the other
destinations keep sending.

Converted destinations go out as H.264 video and AAC sound, which every
platform takes, or as HEVC when the destination asks for it (enhanced RTMP,
which YouTube and current OBS and ffmpeg take). A stream that arrives as H.264
or HEVC can be converted; anything else (AV1) can be sent on as it
is, but not converted.

## Change or remove one

The switch on a tile turns that destination on or off without opening
anything. Off, it stops sending and keeps its key.

Press the tile itself to rename it, change which stream it sends, give it
another server, replace its key or **Remove** it. Leave the key box alone and
the key in use is kept, even when the server changes. **Save** applies it.
Remove asks once, then stops that destination and forgets its key; the
channel and its other destinations are not touched.

## After a restart

Destinations are kept with their channel, their addresses and keys sealed in
the mixer's secret store. When the mixer starts again each one is back as it
was, waiting for the stream, and goes live when the encoder does.

## Where to go next

* [The channel reference](../reference/channels.md#destinations), the three
  methods and the fields a destination reports
* [Stream the programme to a platform](stream-to-a-platform.md), for sending
  the mixer's own output rather than an encoder's stream
