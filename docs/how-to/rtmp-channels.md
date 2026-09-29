# Take streams from several encoders on one port

A channel is a named place encoders publish to. The Sunday service has one,
the youth hall has another, and each can take several streams at once: the
main camera, a second camera, a phone in the gallery. They all arrive on the
mixer's one RTMP port, each needs a key, and each live stream can become a
source by itself.

The Channels page in the web UI that this page describes arrives separately;
until it does, the same things are done through the `channel.*` methods in
[the channel reference](../reference/channels.md).

## Before you start

The listener is the ingest plugin. If Channels says nothing is listening, it
says why and offers the fix: install the ingest plugin from the Plugins page,
or switch it on there. The port is 1935 unless the ingest plugin's settings
say otherwise.

## Make a channel

Press New channel and give it a name, Sunday service say. The mixer makes it,
makes its first key, and shows the key once. Copy it then. The mixer keeps it
sealed and will only ever show its last four characters again; a lost key is
replaced, not recovered.

The channel shows the server address an encoder needs, something like
`rtmp://192.168.1.20:1935/sunday-service`. That is this machine's address on
its network. A phone cannot reach `127.0.0.1`, so use the one shown.

## Point an encoder at it

In OBS: Settings, Stream, Service Custom.

```
Server:      rtmp://192.168.1.20:1935/sunday-service
Stream key:  main?psk=<the key>
```

`main` is the stream's name and can be anything: `main`, `cam2`,
`main_720p`. Give each encoder on the channel a different one. An encoder that
sends its own ladder of renditions sends each as its own stream, and the mixer
takes them all without transcoding any of them.

An encoder that has only one box for the whole address takes
`rtmp://192.168.1.20:1935/sunday-service/main?psk=<the key>`.

Some hardware encoders have one box for the key and nothing else. For those,
set the channel's key mode to Key is the stream name, and type the key alone
in that box. The stream then shows up named after the key's label.

`?key=`, `?token=` and `?Token=` work as well as `?psk=`, for encoders that
insist on one of those.

## Watch it arrive

Within a second or two of the encoder starting, the stream shows as live on
its channel with its size, frame rate and bit rate, and a source called
`sunday-service-main` appears among the sources, ready to put in a scene or
take. A second encoder on `cam2` appears beside it as `sunday-service-cam2`.

If a stream should not become a source by itself, switch off Add live streams
as sources on the channel. The stream is still received and still shown.

## When the encoder stops

A source nobody has put in a scene goes away with its stream. One that is in a
scene stays where it is, shows its last picture, and says it is waiting; when
the encoder comes back the picture comes back with it. Either way the
programme carries on: nothing an encoder does can stop it.

## Give a key to one more person

Add a key to the channel and give that one out, with a label saying who has
it. Taking a key back later cuts off whoever is on air with it and turns them
away from then on, and nobody else notices. Switching a channel off does the
same for every key at once.

## When an encoder is turned away

The encoder's own error box says why, in a sentence: the channel name in the
server address is wrong, the channel is switched off, the key is missing, the
key is not one of the channel's keys, or somebody else is already publishing
that stream name. The same sentence is in the mixer's log and on the channel,
without the key.

## Where to go next

* [The channel reference](../reference/channels.md), every method and field
* [Receive a phone or an OBS stream](receive-a-phone-or-obs-stream.md), for one encoder on a port of its own
* [Install a plugin](install-a-plugin.md)
