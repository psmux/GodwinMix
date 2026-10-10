# The publisher page

A browser's camera and microphone, published to a channel over WHIP. One set
of modules under `ui/join/`, used in two places:

* `/join/` on the control port, a page of its own, told where to publish by
  its link
* the card **This browser's camera** opens in the mixer's page, from Add a
  source or the palette ([how to](../how-to/use-this-browsers-camera.md))

There is no method of its own. It publishes with `POST /whip/<channel>/<stream>`
([WHIP on the control port](channels.md#whip-on-the-control-port)), and the
card in the mixer's page makes its channel with the `channel.*` methods.

## `/join/`

```
GET /join/
GET /join
```

Both answer the page. Everything it needs is in the fragment, which the
browser keeps to itself:

| Key | What it is |
|---|---|
| `whip` | The WHIP address, `/whip/<channel>/<stream>`, or a whole URL on the same origin |
| `channel` | In place of `whip`: the channel's application name. The page publishes to `/whip/<channel>/<stream>` with a stream name of its own (below), and shows a **Name** field to change it |
| `key` | The channel key, sent as `Authorization: Bearer <key>` |
| `title` | What the page and its tab are called. Optional |

```
/join/#whip=/whip/browser/laptop&key=<key>&title=Laptop%20camera
/join/#channel=browser&key=<key>
```

A link with no `key`, or with neither `whip` nor `channel`, says so on the
page and publishes nothing. Neither form needs an operator token: the page
calls no method, and the WHIP endpoint takes the channel key.

The second form is what **A phone's camera** in Add a source shows as a QR
code, on the first `core.info` `tls.urls` address that is not loopback. When
there is none (HTTPS is off, or the control port is bound to loopback) the
dialog says how to change that instead of showing a code.

### The stream name

One per browser, kept in its `localStorage` under `gmx.join.name`:

| | |
|---|---|
| Made up | The browser and its system as a slug, a hyphen and four letters or digits: `safari-ios-m7qd`, `chrome-windows-cd27`. Made the first time it is needed |
| Typed | What the person typed in **Name**, as a slug: lower case letters, digits and single hyphens, at most 40 characters. `Ana's phone` is `anas-phone` |

The field is locked while the page is connecting, live or reconnecting. The
source the stream becomes is `<channel>-<stream>`, made by the channel's
auto source, so every phone is a source of its own. In a private window
with storage blocked the name lasts as long as the page.
The page is served under the same Content Security Policy as the rest of the
UI, and loads nothing from anywhere else.

Remote guests will get their own link with a key of their own; until invites
exist, this link carries the channel's key.

## What it sends

| | |
|---|---|
| Offer | Sent once ICE gathering is complete, with every candidate in it. The endpoint takes no trickle |
| Video | H.264 first in the codec preferences (packetization mode 1, constrained baseline first among those), then VP8, then the rest. One encoding, at most 2.5 Mbps and 30 frames a second, from a camera asked for exactly 1280x720 at 30, cropped and scaled, or 720x1280 on a touch screen held upright; a camera that cannot make that size is asked for it as `ideal`. `degradationPreference` is `maintain-resolution` where the browser lets it be set, so the picture keeps one size and gives up frame rate on a slow link |
| A change of size | A phone turned while live, or a browser that changes size anyway, sends a new H.264 configuration. The channel carries it to the mixer and the decoder there starts again at the new size; the source holds its last picture for a second or so meanwhile |
| VP8 | Taken from a browser that offers no H.264, where the ingest plugin finds `rtpvp8depay`, `vp8dec` and an H.264 encoder (`x264enc`, else `openh264enc`). It is decoded and encoded again as H.264 for as long as that publisher is live. Without those elements the offer is refused as before |
| Audio | Opus, the browser's own. Echo cancellation, noise suppression and automatic gain are on unless the switch on the page is off |
| No camera | The video section stays in the offer with no track, because a channel refuses an offer with no video codec it takes. Nothing is sent on it, and the stream has `video: null` |
| Flip camera | Shown when the browser lists more than one camera. Asks for the other `facingMode` (`user` and `environment`) when the camera says which way it faces, else the next camera in the list. The camera in use is stopped first, because many phones cannot open two; when nothing else opens, it is opened again. On the air it is a `replaceTrack`, as any other camera change is |
| Screen wake lock | `/join/` only. `navigator.wakeLock.request("screen")` while connecting, live or reconnecting, asked again on `visibilitychange` when the page is visible, since a hidden page loses it. Released on Stop. A browser without it publishes the same and lets the screen sleep |
| Stop | `DELETE` on the `Location` the `201` gave |

Muting the camera or the microphone sets the track's `enabled` to false: the
stream carries black or silence and nothing is renegotiated. Another device
is put on the same sender with `replaceTrack`.

## States

| State | When |
|---|---|
| Not publishing | The page is open and Go live has not been pressed |
| Connecting | The offer is on its way, or the connection is being made |
| Live | The peer connection is `connected` |
| Reconnecting | The connection failed, or stayed `disconnected` for five seconds, or the offer was refused with anything but the codes under Stopped (`400`, `404`, `409`, `5xx`, the network). It tries again after 1, 2, 4 and 8 seconds, then every 15, for as long as the page is open |
| Stopped | Stop was pressed, or the offer was refused with `401`, `403` or `415`, or the browser can send neither H.264 nor VP8 |

The sentence the endpoint answered with is shown as it came.

A `404` is retried because a mixer or a station that is restarting answers it
for a moment before its channels are back. A channel that really has gone
shows its sentence under Reconnecting every fifteen seconds until somebody
presses Stop.

## Coming back after a lost network

| Event | What the page does |
|---|---|
| `disconnected` | Waits five seconds for it to come back by itself, then offers again |
| `failed` or `closed` | Offers again at once, and after the waits above if that fails |
| `online` on `window` | Ends the current wait and offers again now |
| `change` on `navigator.connection` (Wi-Fi to cellular, a new access point; Chrome on Android) | The same, and a connection that reads `disconnected` in the next fifteen seconds is not waited for, because its path went with the old network |
| `visibilitychange` to visible | Ends the current wait and offers again now. A hidden page's timers run once a minute at most |

A connection that is `connected`, or an offer already on its way, is left
alone by all of these.

There is no ICE restart: the endpoint answers `PATCH` with `405`. A new offer
starts a new session, which takes over the old one at once, because by then
the old has sent nothing for more than two seconds. The mixer keeps its end of
a `disconnected` session for fifteen seconds, longer than the page waits, so
it never ends a session the page still expects to come back; it ends one at
once on `failed` or `closed`.

## What runs, and when

| Work | Runs while |
|---|---|
| The preview | The card or page is on screen, unfolded, and the tab is visible |
| The level meter | The same. One `AnalyserNode`, read once a frame; its audio context is suspended otherwise |
| The stats line | The same, and the publish is live. `getStats` once a second |

Nothing polls the mixer. The card in the mixer's page learns that its source
exists from `event/channel.changed` and the status the page already has.

## The card's channel

The card in the mixer's page publishes to the channel `browser`:

1. `channel.get {id: "browser"}`. A `-32004` means there is none, and
   `channel.add {name: "Browser", app: "browser", auto_source: true,
   protocols: ["whip"]}` makes it and answers its key.
2. Otherwise, when it is switched off, has `auto_source` off, has `key_mode`
   other than `query`, or does not take WHIP, one `channel.set` puts those
   right and leaves the rest.
3. `channel.key.reveal` with its first key, or `channel.key.add` when it has
   none.

The stream name is the browser's own, as on `/join/` ([The stream
name](#the-stream-name)): `chrome-macos-k3f9`. The source is
`browser-<stream>`.

`channel.get` needs the read scope and the others need admin, so the card
works for an operator whose token is an admin one. **A phone's camera** makes
and reads the channel the same way, then reads `core.info`; the phone itself
needs no token.

## On a phone

At `/join/` the controls are at least 48 pixels tall, text fields use a 16
pixel font so Safari does not zoom into them, nothing depends on hover, and
at 480 pixels wide or less the buttons sit two to a row with Go live across
the whole width. The picture box takes the shape of the picture, upright or
on its side, and is never taller than 45% of the screen, so the controls stay close under it on a phone held upright.
