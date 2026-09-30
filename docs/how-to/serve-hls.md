# Serve HLS to viewers

## From the page

HLS lets anyone on your network watch the programme on a phone, a laptop or a
smart TV, with no platform and no stream key. It is served from the mixer's own
port, so there is nothing to open on the router.

### Start it

1. Press **Add destination** on the Outputs panel.
2. Press **HLS for viewers** at the end of the list. It is only there when this
   mixer can serve HLS.
3. Give it a short name. The name becomes part of the link, so `viewers` gives
   `/hls/viewers/master.m3u8`.
4. Pick the sizes a viewer's player can switch between. **Four sizes** is
   1080p, 720p, 480p and 360p, **Three sizes** leaves out 1080p. Each card draws
   its sizes to scale and says how much goes out in all. **Custom** lets you
   add and remove sizes and set each one's bitrate.
5. Tick **Low latency** if the people watching are in the same room and the
   delay matters. Players that do not support it still play, a few seconds
   further behind.
6. Press **Start serving**.

Every size is its own encode, so a ladder of four costs more than one
destination. If the machine has no room for it, the form says what it needs
and offers what fits, as for any other destination.

### Share the link

Once it is live, the HLS row on the Outputs panel has a card with the link,
a **Copy** button and a QR code. Scan the QR code with a phone on the same
network and it plays. When the page is open on the mixer itself, the link uses
the machine's network address rather than 127.0.0.1, so a phone can reach it.

**Watch here** opens a small preview in the page. Safari, the desktop app on a
Mac and recent Chrome play HLS by themselves. A browser that cannot gets an
**Open in a player** link instead, and a sentence saying why: the page carries
no player of its own, because the smallest one would add about 300 kB to it.

The link carries the output's viewer key, never the control token. The key
lets a player read this one output's playlists and segments and nothing else
on the mixer, and it stays the same when the mixer restarts, so a link on a
poster keeps working. If a link gets somewhere it should not, remove the
output and add it again under a new name.

## From the API

The same thing as one call:

```sh
curl -X POST http://127.0.0.1:8080/api/v1/outputs \
  -H "authorization: Bearer $GODWINMIX_TOKEN" -H 'content-type: application/json' \
  -d '{"id":"viewers","type":"hls/output","uri":"hls://viewers",
       "rendition":{"preset":"abr-ladder-4"},"params":{"low_latency":true}}'
```

Leave `rendition` out to serve the programme exactly as it is encoded, one
size, which costs almost nothing. `docs/reference/hls-output.md` has every
param.

The link is in the output's status:

```sh
curl -s -H "authorization: Bearer $GODWINMIX_TOKEN" \
  http://127.0.0.1:8080/api/v1/outputs/viewers | jq -r .playback.master_url_path
# /hls/viewers/master.m3u8?key=68ydq4dbicw3d8nhsf42ca9c
```

Put the address people reach the mixer on in front of it.

## Play it

Any HLS player opens the link: Safari, VLC, a smart TV, `ffplay`, or hls.js
on a web page of your own. To check it from the mixer's own machine:

```sh
ffprobe "http://127.0.0.1:8080/hls/viewers/master.m3u8?key=..."
ffplay "http://127.0.0.1:8080/hls/viewers/master.m3u8?key=..."
```

With low latency on, hls.js (with `lowLatencyMode`, its default) and Safari
play about two seconds behind the camera. A player that does not know LL-HLS
plays the same link a few seconds further back.

A web page on another site can play it too: the control port answers
cross origin requests.

The control port speaks HTTP/1.1. Apple's LL-HLS specification asks for
HTTP/2, so to give Safari and iPhones low latency, put the mixer behind a
reverse proxy with HTTPS (`docs/how-to/reverse-proxy.md`). hls.js needs
nothing more: it keeps two or three requests open, well inside the six a
browser allows one host.

## The same thing as DASH

A player that wants DASH opens `/hls/viewers/manifest.mpd?key=...` instead
(`playback.dash_url_path` in the output's status). It lists the very same
segments, so it costs nothing more to serve; dash.js plays it about six
seconds behind.

## See who is watching

`viewers` on the output's row counts players that fetched something in the
last two windows (a minute, by default), and `egress_kbps` is what they are
pulling between them. Every viewer adds that rung's bitrate to your upload,
so on a home connection a few dozen viewers of 1080p is the limit, whatever
the mixer can do.
