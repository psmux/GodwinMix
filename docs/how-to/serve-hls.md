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

The link carries no token. A mixer that asks for a token on its control port
asks for one on the link as well, and a player that cannot send one will not
play it.
