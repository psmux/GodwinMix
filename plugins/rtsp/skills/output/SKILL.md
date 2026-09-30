---
name: rtsp-output
description: Serve the GodwinMix programme as an RTSP stream that hardware decoders, network video recorders, surveillance walls, VLC, OBS or another mixer pull from an address. Use when the operator asks for an rtsp:// address to give to something, mentions an NVR, a decoder or a player that "pulls", or wants the programme on a device that cannot receive a push.
---

# rtsp/output

Serves the programme at `rtsp://<this machine>:<port>/<path>`. The port opens
when the output starts and closes when it is removed; nothing listens before.
The programme's encode is packetised as it is; nothing is encoded again, and
every player shares one packetiser.

## Adding one

```
output.add {id: "rtsp", type: "rtsp/output", uri: ""}
output.add {id: "nvr", type: "rtsp/output", uri: "", port: 8554, path: "studio/main"}
```

Players then open `rtsp://<this machine>:8554/live` (or the path given), for
example `vlc rtsp://192.168.1.10:8554/live` or
`ffplay -rtsp_transport tcp rtsp://192.168.1.10:8554/live`.

## The settings that matter

* `port` (8554): the TCP port. 554 is the standard one and needs the machine's
  administrator on Linux and macOS.
* `path` (`live`): what follows the port in the address.
* `bind` (`0.0.0.0`): `127.0.0.1` keeps it to this machine.

## Things to know

* A player that joins starts at the next keyframe, so up to one keyframe
  interval of black before the picture.
* Players behind a firewall should ask for TCP (`-rtsp_transport tcp` in
  ffmpeg, "RTP over RTSP (TCP)" in VLC); UDP needs the player's ports open.
* H.264 and H.265 video, AAC, MP3 and Opus audio. The programme is H.264 and
  AAC unless the output asks for a rendition.
* This output reads the programme from a Unix FIFO, so it runs on Linux and
  macOS.
