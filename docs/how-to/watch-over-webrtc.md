# Watch the programme over WebRTC

A WebRTC viewer sees the programme less than half a second behind, in a
browser tab or any player that speaks WHEP (GStreamer's `whepsrc` and
`whepclientsrc`, an app built on a WebRTC library). It is served from the
mixer's own address, so there is no port to open for signalling. Use it for a
confidence monitor in another room, a director on a laptop, or a remote guest
who has to react to what is on air. For a large audience use
[HLS](serve-hls.md), which scales and WebRTC does not.

## Add the output

1. Press the palette button (the `⌘K` at the top right) and choose
   **Add an output**.
2. Press **WebRTC viewers (WHEP)**.
3. Give it a short name, such as `monitor`. Viewers open `/whep/monitor`.
4. Leave **Most viewers** at 10 unless you know you need more. Every viewer is
   another encrypted copy of the same encode, which costs upload and a little
   CPU, never another encoder.
5. Open **Advanced** only if viewers are outside this network: put a STUN
   server there, such as `stun://stun.l.google.com:19302`. Empty keeps every
   connection on this network.
6. Press **Start sending**.

The output appears in the Outputs list and goes live once the programme
encoder is running. It does nothing further until somebody watches.

## Give a viewer the address

The output's status carries the path with its viewer key:

```bash
curl -s http://localhost:8080/api/v1/outputs/monitor | grep whep_path
```

answers `"whep_path":"/whep/monitor?key=v6ky6eqehr98v2wwrdxdbfe9"`. Put the
mixer's address in front of it and hand that out as the WHEP endpoint. The key
lets the holder watch this one output and nothing else. It stays the same
after a restart; to change it, give the output a new name or set `viewer_key`
on it.

A client holding a control token with the read scope needs no key, and
`/whep/program` plays the first WHEP output there is.

## Watch in a browser

Any page can play it with a few lines, because WHEP is one POST:

```js
const pc = new RTCPeerConnection();
pc.addTransceiver("video", { direction: "recvonly" });
pc.addTransceiver("audio", { direction: "recvonly" });
pc.ontrack = (e) => { video.srcObject = e.streams[0]; };
await pc.setLocalDescription(await pc.createOffer());
// Wait for ICE gathering to finish: the mixer takes no trickled candidates.
await new Promise((r) => (pc.onicegatheringstatechange = () => pc.iceGatheringState === "complete" && r()));
const res = await fetch(endpoint, { method: "POST", headers: { "Content-Type": "application/sdp" }, body: pc.localDescription.sdp });
await pc.setRemoteDescription({ type: "answer", sdp: await res.text() });
```

`res.headers.get("Location")` is the session. A `DELETE` on it ends the
viewer's session at once; a viewer that simply closes the tab is let go within
a few seconds.

## Send a rendition instead of the programme encode

A WHEP output takes a `rendition` like any other output, so a monitor on a slow
link can have 720p at 2 Mbit/s while the stream to the platform stays 1080p.
The rendition's video may be H.264, H.265, AV1, VP8 or VP9; the viewer's
browser has to offer the same codec, and is refused with a 406 naming it when
it does not.

## When it does not play

* **501, the message names libnice.** WebRTC needs `webrtcbin` and libnice's
  GStreamer elements. On macOS with Homebrew that is
  `brew install libnice-gstreamer`; on Debian and Ubuntu
  `gstreamer1.0-nice`. Restart the mixer after installing.
* **503, already has its viewers.** Raise **Most viewers** on the output, or
  send the audience to HLS.
* **The page connects and shows nothing across the internet.** Set a STUN
  server on the output, and a TURN server if both ends are behind strict NAT.

Every route, code and param is in the
[streams reference](../reference/streams.md#post-whep-target).
