---
name: icecast
description: Send the GodwinMix programme's sound to an Icecast or SHOUTcast 2 server as internet radio, or play an internet radio station as a source. Use when the operator mentions internet radio, an audio only stream, a listen line, Icecast, SHOUTcast, a mount point, a source password, or gives an icecast:// address or a station's stream URL.
---

# icecast/output and icecast/source

## Sending the programme's sound out as radio

```
output.add {id: "radio", type: "icecast/output", uri: "icecast://source:password@radio.example.com:8000/live.mp3"}
output.add {id: "listen", type: "icecast/output", host: "radio.example.com", port: 8000, mount: "church.mp3", password: "...", format: "mp3", bitrate_kbps: 128}
```

The streaming host gives the server, port, mount and source password.
`format` is `mp3` (plays everywhere), `vorbis` or `opus`. The picture is not
sent; the sound is encoded once for the mount, a few percent of one core.

## Playing a station as a source

```
source.add {id: "station", type: "icecast/source", uri: "http://radio.example.com:8000/live.mp3"}
```

The address is the stream, not the station's web page. The source's health
carries the song title the station sends.

## Things to know

* A wrong password is refused by the server; health says so.
* Nothing is listed in the server's public directory unless `public` is true.
