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
| `key` | The channel key, sent as `Authorization: Bearer <key>` |
| `title` | What the page and its tab are called. Optional |

```
/join/#whip=/whip/browser/laptop&key=<key>&title=Laptop%20camera
```

A link with no `whip` or no `key` says so on the page and publishes nothing.
The page is served under the same Content Security Policy as the rest of the
UI, and loads nothing from anywhere else.

Remote guests will get their own link with a key of their own; until invites
exist, this link carries the channel's key.

## What it sends

| | |
|---|---|
| Offer | Sent once ICE gathering is complete, with every candidate in it. The endpoint takes no trickle |
| Video | H.264 first in the codec preferences (packetization mode 1, constrained baseline first among those). One encoding, at most 2.5 Mbps and 30 frames a second, from a camera asked for 1280x720 at 30. `degradationPreference` is `maintain-framerate` where the browser lets it be set |
| Audio | Opus, the browser's own. Echo cancellation, noise suppression and automatic gain are on unless the switch on the page is off |
| No camera | The video section stays in the offer with no track, because a channel refuses an offer with no H.264. Nothing is sent on it, and the stream has `video: null` |
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
| Reconnecting | The connection failed, or stayed `disconnected` for five seconds, or the offer was refused with something waiting may cure (`409`, `5xx`, the network). It tries again after 1, 2, 4 and 8 seconds, then every 15 |
| Stopped | Stop was pressed, or the offer was refused with `400`, `401`, `403`, `404` or `415`, or the browser cannot send H.264 |

The sentence the endpoint answered with is shown as it came.

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

The stream name is the browser and its system as a slug: `chrome-macos`,
`edge-windows`, `firefox-linux`, `safari-ios`. The source is
`browser-<stream>`.

`channel.get` needs the read scope and the others need admin, so the card
works for an operator whose token is an admin one.
