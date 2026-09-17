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

## Add the source

```sh
gmx ctl source add phone --type ingest/rtmp
```

That is the whole configuration. It listens on TCP port 1935, the port every
encoder offers by default, and takes the first publisher who arrives whatever
name and key they use.

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

Within a second or two of the publisher's first keyframe the source turns live
and the log says who it is:

```
live/phone from 10.0.0.31:51666 started publishing
```

Then take it:

```sh
gmx ctl take phone
```

## Try it without anybody else

```sh
dev/harness/publish.sh rtmp 1935 &
```

publishes SMPTE bars and a 440 Hz tone to the same address from this machine, so
you can prove the path before asking somebody to point a real encoder at it.

## Two people at once

One source is one picture, so a second publisher is refused while the first is
live and their own error box says why. For two at once, add a second source on
another port:

```sh
curl -s -X POST localhost:8080/api/v1/sources \
  -H 'content-type: application/json' \
  -d '{"id":"laptop","uri":"ingest/rtmp","type":"ingest/rtmp","params":{"port":1936}}'
```

and give that person `rtmp://<address>:1936/live`.

`gmx ctl source add` carries an id, an address, a `--type` and a name, and has
no flag for a plugin's own settings, so a source that needs them is added over
the API. `uri` is required there: the type id goes in it when a source has no
address of its own, which is what the command line puts there too.

## A stream key that is a password

An empty `stream_key` takes anybody. Setting one makes this source take only the
publisher who knows it, which is the closest RTMP has to a password:

```sh
curl -s -X POST localhost:8080/api/v1/sources \
  -H 'content-type: application/json' \
  -d '{"id":"phone","uri":"ingest/rtmp","type":"ingest/rtmp",
       "params":{"app":"live","stream_key":"nine-fat-owls"}}'
```

Anyone publishing under a different key is refused with a message naming the
one that was wanted. The key is stored encrypted and never read back, but RTMP
itself sends it in the clear: across the internet, use SRT with a passphrase
instead.

## Over SRT rather than RTMP

SRT is the better choice over a link that loses packets, because it asks for
lost packets again inside a delay budget you choose. The listener for it is not
in `ingest`: `srt/source` is one already, and `listener` is its default mode.

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

## What is not here yet

**Sources that appear by themselves.** `ingest/discover` holds one port for many
publishers and reports each one as a source ready to add, with an
`event/ingest.publisher` notification and an `add_publishers` tool. The core
cannot use any of it yet: it starts no `device` provides, calls `discover` from
nowhere, and drops a plugin's `event` notifications into a buffer nothing reads.
`tool.call` itself is registered, so `add_publishers` would be reachable once a
`ingest/discover` instance runs. `plugins/ingest/src/device.rs` names the gaps
with the file that would change for each.

Until they land, an `ingest/rtmp` source that owns its own port does the job:
added once, every publisher who arrives afterwards is live within seconds with
nothing configured at either end.

**An RTSP server.** Pulling *from* an RTSP camera works today with a bare
`rtsp://` address. Running an RTSP server that cameras push to would mean
linking `libgstrtspserver-1.0`, which would break this plugin's build for people
who only wanted RTMP, so it belongs in a plugin of its own with its own platform
list.

## When nothing arrives

1. `gmx ctl status` shows the source `degraded` with the address to use. Read it
   back to the person publishing, port and all.
2. The address must be the one on the network. `godwinmix --info` prints it.
3. A firewall between the two machines. RTMP is TCP on 1935; SRT is **UDP** on
   its port, so a TCP rule will not do.
4. A `stream_key` set here refuses a publisher using a different one, and the
   refusal reaches their error box.
5. A port below 1024 needs privileges this process usually does not have, and
   the error says so.

## Where to go next

* [Receive and send SRT](srt.md)
* [Send the programme to a WHIP endpoint](send-to-whip.md)
* [The network plugins, every setting](../reference/plugins-network.md)
* [Install a plugin](install-a-plugin.md)
