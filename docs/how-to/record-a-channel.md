# Record a channel and share a watch link

A channel can keep what its encoder sends on this machine, two ways, beside
or instead of sending it on to a platform:

* **Record** saves the stream to a file on the mixer.
* **Watch link** serves it as HLS from the mixer's own address, a link anyone
  on your network can open on a phone, a laptop or a smart TV, with no
  platform and no stream key.

Both copy the stream as the encoder sends it. Nothing is decoded or encoded,
so neither costs the machine an encode, and both run only while switched on.

## Record

On the Channels tab, the channel's **Send on to** strip offers **Record**
beside the platforms (behind **Add** once the channel has a destination). The
form says what it saves, for instance:

> Saves sunday-service-main-20261009-103000.ts, copied, not converted.

Leave **Folder on the mixer** empty to save into `Videos/GodwinMix` in the
home folder of the user the mixer runs as, the folder a programme recording
uses, or type another folder. Press **Start recording**.

Each time the encoder goes live a new file starts, named for the channel, the
stream and the local time it started:
`sunday-service-main-20261009-103000.ts`. While it records, the tile says
`Recording, 12.3 MB, 4:05`, with the file's name under it; hover the name for
the whole path. When the encoder stops, the file is closed and the tile keeps
its name, size and length until the next one starts. Switch the tile off to
stop recording; the file is closed cleanly.

The file is MPEG-TS, which VLC and ffmpeg open as it is. A file cut short by
a power cut still plays, because there is no index to finish.

The same from a terminal:

```sh
curl -X POST http://127.0.0.1:8080/api/v1/channels/sunday-service/destination/add \
  -H "authorization: Bearer $GODWINMIX_TOKEN" -H 'content-type: application/json' \
  -d '{"platform":"file"}'
```

Send `"server": "D:/Recordings"` to record somewhere else.

## Share a watch link

Press **Watch link** on the same strip, then **Make the link**. Once the
encoder is live, a card under the tiles has:

* the link, with **Copy**;
* a QR code, which a phone on the same network scans to play it;
* **Watch here**, a preview in the page where the browser plays HLS;
* **Embed on a web page**: a `<video>` and a few lines that play the link on
  a page of your own, with hls.js from a CDN for browsers that need it.

The link looks like

```
http://192.168.1.20:8080/hls/channel/sunday-service/watch-link/index.m3u8?key=...
```

The key in it opens this one link and nothing else on the mixer, so the link
is safe to put on a poster. It stays the same when the mixer restarts. The
tile counts the players watching: `Live, 3 watching`.

When the page is open on the mixer itself, the link uses the machine's
network address rather than 127.0.0.1, so a phone can reach it.

A watch link copies the stream, so the encoder has to send what HLS carries:
H.264 (or HEVC) and AAC, which is what OBS and nearly every encoder send. A
stream with other sound, MP3 or Opus for instance, turns the tile red with a
sentence naming the codec; set the encoder to AAC.

The same from a terminal:

```sh
curl -X POST http://127.0.0.1:8080/api/v1/channels/sunday-service/destination/add \
  -H "authorization: Bearer $GODWINMIX_TOKEN" -H 'content-type: application/json' \
  -d '{"platform":"hls"}'
```

The link is the destination's `playback.master_url_path` in `channel.get`;
put the address people reach the mixer on in front of it. To make segments
shorter, for viewers in the same room, send
`"server": "hls://?segment_ms=1000&low_latency=true"`.

The packaging runs in the station's HLS packager, the process that serves a
show's HLS output, started while a watch link is on. If it stops, the tile
says so and it comes back on the same link within a few seconds.

[The channel reference](../reference/channels.md#record-and-watch-link) has
every field.
