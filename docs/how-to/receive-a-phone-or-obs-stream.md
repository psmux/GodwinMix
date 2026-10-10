# Receive a phone or an OBS stream

Somebody with a phone, a laptop running OBS, or a hardware encoder in a rack
needs to get their picture into the mixer. This page makes the mixer the server
they send to, so they type an address and nothing else.

This takes about five minutes and needs no other software. Until now a setup
like this needed mediamtx or nginx-rtmp running beside the mixer; the `ingest`
plugin is that, built in.

## Install the plugin

```sh
dev/harness/stage-plugins.sh
gmx plugin add ./plugins/ingest
```

`stage-plugins.sh` builds the first party network plugins and puts each binary
beside its manifest, which is where `gmx plugin add` copies it from. From a
release build the plugin is already staged and this step is not needed.

The RTMP server itself is written in Rust, so nothing has to be installed beside
the mixer. GStreamer is still needed for the remux that hands the stream to the
core, and every element it uses ships with GStreamer itself.

## Nothing to add

That is the whole configuration. With the plugin installed the mixer listens
on TCP port 1935, the port every encoder offers by default, and while it has
no channels it takes whoever publishes, whatever name they use, and makes each
of them a source.

## Tell them where to send

```
Server:     rtmp://<the mixer's address>:1935/live
Stream key: anything
```

The mixer's address is its address on the network, not `localhost`: a phone
cannot reach `127.0.0.1`. `godwinmix --info` prints the addresses it is bound
to.

In OBS that is Settings, Stream, Service: Custom, and those two boxes. On a
phone app it is usually one box taking the whole URL,
`rtmp://<address>:1935/live/phone`.

## Watch it arrive

```sh
gmx ctl status
```

Within a second or two of the publisher's first keyframe a source named after
the path, `live-phone`, is there and live, and the log says who it is:

```
live/phone from 10.0.0.31:51666 started publishing
```

Then take it:

```sh
gmx ctl take live-phone
```

When they stop publishing the source goes again.

## Try it without anybody else

```sh
dev/harness/publish.sh rtmp 1935 &
```

publishes SMPTE bars and a 440 Hz tone to the same address from this machine, so
you can prove the path before asking somebody to point a real encoder at it.

## Two people at once

The port takes several publishers at once. A laptop on `live/laptop` beside
the phone on `live/phone` is a second source, `live-laptop`, and neither
disturbs the other. Two publishers on the same name are one too many: the
second is refused while the first is live, and its own error box says why.

## Keys, so not just anybody can publish

While there are no channels the door is open to anyone who can reach the
port. Make a channel and only its encoders, with its keys, get in: [Take
streams from several encoders on one port](channels.md). Each key can be
given to one person and taken back without touching the others. RTMP itself
sends the key in the clear, so across the internet use SRT with a passphrase
instead.

## A source on a port of its own

An `ingest/rtmp` source can also own a port by itself, one publisher at a
time, with its own application name and stream key. It cannot share a port
with the channel server, so give it another one:

```sh
curl -s -X POST localhost:8080/api/v1/sources \
  -H 'content-type: application/json' \
  -d '{"id":"guest","uri":"ingest/rtmp","type":"ingest/rtmp",
       "params":{"port":1936,"app":"live","stream_key":"nine-fat-owls"}}'
```

and give that person `rtmp://<address>:1936/live` and the key. Anyone
publishing under a different key is refused with a message naming the one
that was wanted. The key is stored encrypted and never read back.

When the publisher's network drops, nobody has to do anything. A connection
that has sent nothing for 5 seconds is closed, and one that has sent nothing
for 2 seconds is cut off as soon as another publisher arrives. Either way the
source ends its stream and the mixer starts it again on a clean one within a
second, holding the last picture meanwhile, so the encoder's reconnect goes
straight to air. A newcomer that arrives during that second is asked to
publish again, which an encoder set to reconnect does by itself. The same is
true of an `ingest/whip` source: a browser that publishes to it again gets a
clean stream.

`gmx ctl source add` carries an id, an address, a `--type` and a name, and has
no flag for a plugin's own settings, so a source that needs them is added over
the API. `uri` is required there: the type id goes in it when a source has no
address of its own, which is what the command line puts there too.

## Over SRT rather than RTMP

SRT is the better choice over a link that loses packets, because it asks for
lost packets again inside a delay budget you choose. A channel can take SRT
on the mixer's one SRT port, with the channel's key as the passphrase: switch
SRT on in its settings ([Take streams from several encoders into one
channel](channels.md)). For one feed and no channel, `srt/source` listens on a
port of its own:

```sh
gmx plugin add ./plugins/srt
gmx ctl source add feed "srt://0.0.0.0:9000?mode=listener&latency=300" --type srt/source
```

They send to `srt://<the mixer's address>:9000` in caller mode.
[Receive and send SRT](srt.md) has the rest of it.

## From a browser, with no app at all

`ingest/whip` runs a WHIP endpoint, so a guest opens a page, allows their
camera, and publishes over WebRTC:

```sh
gmx ctl source add guest --type ingest/whip
```

They publish to `http://<the mixer's address>:8889/whip`. It needs
`whipserversrc`, which arrived in the GStreamer 1.28 rs webrtc set; on an older
build the source refuses to start with a message naming the package.

For the camera on the computer where the mixer's page is open, there is a
button for it: [Use this browser's camera and
microphone](use-this-browsers-camera.md). It publishes into a channel by WHIP,
on the mixer's own port.

## What is not here yet

**An RTSP server.** Pulling *from* an RTSP camera works today with a bare
`rtsp://` address. Running an RTSP server that cameras push to would mean
linking `libgstrtspserver-1.0`, which would break this plugin's build for people
who only wanted RTMP, so it belongs in a plugin of its own with its own platform
list.

## When nothing arrives

1. A source appears only once somebody publishes. Nothing in `gmx ctl status`
   means nothing has arrived; check the address they were given, port and all.
2. The address must be the one on the network. `godwinmix --info` prints it.
3. A firewall between the two machines. RTMP is TCP on 1935; SRT is **UDP** on
   its port, so a TCP rule will not do.
4. Once a channel exists, a publisher with no key or a wrong one is refused,
   and the refusal reaches their error box and the mixer's log.
5. A port below 1024 needs privileges this process usually does not have, and
   the error says so.

## Where to go next

* [Use this browser's camera and microphone](use-this-browsers-camera.md)
* [Receive and send SRT](srt.md)
* [Send the programme to a WHIP endpoint](send-to-whip.md)
* [The network plugins, every setting](../reference/plugins-network.md)
* [Install a plugin](install-a-plugin.md)
