# RTMP channels: the contract

Written before the work starts, so the backend and the UI can be built at the
same time against one shape. A change to it is made here first, in the same
commit as the code that needs it.

## What a channel is

A channel is a named place encoders publish to, on the mixer's own RTMP port,
the way a Livebox channel was an nginx-rtmp `application`:

    rtmp://<mixer>:1935/<app>/<stream>?psk=<key>

* `<app>` is the channel's slug (`sunday-service`). One port, many channels.
* `<stream>` is any name the encoder picks (`main`, `main_720p`, `cam2`).
  Several streams under one channel may publish at once. That is how an
  encoder that sends its own ladder (MBR) keeps working: each rendition is a
  stream, and the mixer does not transcode anything.
* The key may arrive as `?psk=`, `?key=`, `?token=` or `?Token=` on the stream
  name, or as the whole stream name when the channel is set to "key is the
  stream name". A channel has one or more keys, each with a label, so a key
  given to one person can be taken back without touching the others.
* A publisher with no key, or a wrong one, is refused with
  `NetStream.Publish.Denied` and a reason, and the refusal is logged.

A live stream can become a mixer source by itself (`auto_source`, on by
default), named `<app>-<stream>`, removed again when the publisher leaves
unless it is in a scene. A channel's streams can also be sent straight on to
destinations (YouTube, Facebook, Twitch, any RTMP or SRT) as they arrive, by
remuxing, never by decoding: that is distribution without the mixer in the
path, what Livebox's push did.

## Methods

Admin scope for anything that writes, read scope for list. Ids are slugs.
Every refusal says the state and the next step and carries `data`, with an
`action` where a button fits (see docs/reference/errors.md).

    channel.list {}                          -> {channels: [Channel], rtmp: {port, urls: ["rtmp://10.0.0.5:1935", ...]}}
    channel.get {id}                         -> Channel
    channel.add {name, app?, auto_source?, key_mode?}
                                             -> {channel: Channel, key: {id, label, secret}}   secret shown once
    channel.set {id, name?, app?, enabled?, auto_source?, key_mode?}   -> Channel
    channel.remove {id}                      -> {removed: id}
    channel.key.add {id, label?}             -> {key: {id, label, secret}}                     secret shown once
    channel.key.remove {id, key}             -> Channel
    channel.destination.add {id, platform, label?, server?, key?, stream?, enabled?}  -> Channel
    channel.destination.set {id, destination, label?, server?, key?, stream?, enabled?} -> Channel
    channel.destination.remove {id, destination} -> Channel

`key_mode` is `"query"` (default: the key rides as `?psk=` or its aliases) or
`"stream"` (the stream name is the key, for encoders with one box).

## Shapes

    Channel {
      id, name, app, enabled, auto_source, key_mode,
      keys: [{id, label, created, hint}],          hint: last four characters, never the key
      publish: {server: "rtmp://<first address>:1935/<app>", example: "<server>/main?psk=<key>"},
      streams: [Stream],
      destinations: [Destination],
    }
    Stream {
      name, state: "live" | "idle", since_ms, from,      from: the publisher's address
      key: <key id that let it in>,
      video: {codec, width, height, fps, kbps} | null,
      audio: {codec, channels, sample_rate, kbps} | null,
      source: <mixer source id> | null,
    }
    Destination {
      id, platform, label, uri_host, has_key, stream,   stream: which stream to send, "*" is the first live one
      enabled, state: "off" | "waiting" | "connecting" | "live" | "reconnecting" | "failed",
      since_ms, kbps, reconnects, error | null,
    }

`platform` is an id from ui/client/destinations.js PLATFORMS (youtube,
facebook, twitch, custom, srt) plus any added there. Keys are write only:
lists carry `has_key`, never the key.

## Events

    event/channel.changed   {channel: Channel}          any change to a channel, its streams or its destinations
    event/channel.removed   {id}
    event/channel.refused   {id, stream, from, why}     a publisher turned away

Clients subscribe with `core.subscribe` patterns `channel.*`, like every
other event. Nothing is computed for a channel nobody is looking at beyond
what the server does anyway to serve its publishers.

## Performance rules

* One listener task, one task per connection, no lock held across an await,
  no allocation per media message on the hot path (tags are `Bytes`, shared).
* A publisher is never slowed by a reader. Every consumer (a mixer source, a
  destination) reads through a bounded queue; a consumer that falls behind
  loses whole GOPs from the front and restarts at the next keyframe, and the
  loss is counted, not waited on.
* A destination is a remux of the publisher's own bytes. No decode, no
  encode, no GStreamer on that path.
* A dead destination reconnects with the backoff the outputs already use and
  never touches the publisher or the other destinations.
* Channels persist in the runtime overlay beside the config, as sources and
  outputs do, and survive a restart.
