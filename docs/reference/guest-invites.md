# Guest invites

An invite is a link with a key of its own. Whoever opens it publishes their
camera and microphone into one show, as one source, and sees and hears a
return made for them: the programme picture, with the sound of everyone on
air but themselves. The key does nothing else. It is not a channel key and
not a control token, so a guest who forwards the link gives away one seat
in the show, until the invite is revoked or runs out. How to use it is in
[invite a guest](../how-to/invite-a-guest.md).

## Methods

Every method is on `/rpc` and over REST at the path its name makes
(`guest.invite.create` is `POST /api/v1/guest/invite/create`). The answers
are the shapes in [protocol.md](../../protocol.md).

| Method | Scope | Params | Answers with |
|---|---|---|---|
| `guest.invite.create` | admin | `name`, and `id`, `expires_in_secs`, `base`, each optional | the invite, `link`, `qr_svg`, and `note` when the link would not open on another device |
| `guest.invite.list` | read | none | `guests`, `talkback`, `max_guests` |
| `guest.invite.link` | admin | `id` | `link`, `qr_svg`, `note` |
| `guest.invite.revoke` | admin, destructive | `id` | `{"revoked": <the invite>}` |
| `guest.talkback.set` | operate | `on`, and `guest`, `source`, each optional | the same as `guest.invite.list` |

Making and revoking an invite is admin because an invite is a credential:
it lets somebody put a camera into the show. On a mixer with no token every
caller is admin.

### `guest.invite.create`

| Param | Default | What it is |
|---|---|---|
| `name` | required | What the guest is called on screen and in the desk. 1 to 64 characters |
| `id` | the name as a slug | Lower case letters, digits and single dashes, at most 40. `-2`, `-3` go on the end of a made id that is taken. `talkback` is the operator's and no invite can have it |
| `expires_in_secs` | 86400, a day | 60 to 2592000, thirty days. When it runs out the guest is let go and the link stops working |
| `base` | this machine | The start of the link, such as `https://studio.example.com`, for a mixer reached through a name or a tunnel |

```sh
curl -sk -H "Authorization: Bearer $ADMIN" -H "Content-Type: application/json" \
  -d '{"name": "Ana Lopez"}' https://192.168.1.20:8080/api/v1/guest/invite/create
```

```json
{
  "invite": {
    "id": "ana-lopez", "name": "Ana Lopez",
    "created": "2026-10-29T18:02:11.204Z", "expires": "2026-10-30T18:02:11.204Z",
    "state": "invited", "source": "guest-ana-lopez", "bus": "guest-ana-lopez", "talkback": false
  },
  "id": "ana-lopez",
  "link": "https://192.168.1.20:8080/join/#guest=ana-lopez&key=k3f9...&title=Ana%20Lopez",
  "qr_svg": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 53 53\" ...</svg>"
}
```

The key is 32 characters from a 32 letter alphabet, 160 bits. The link is
the `/join/` page with the guest's id, the key and the name in the fragment,
which a browser never sends to a server. Under a station it carries
`&show=<id>` as well, so the page reaches this show.

The host in the link is the first https address in `core.info` `tls.urls`
that is not loopback, the one the phone camera code uses. When there is
none, `note` says why and what to change:

* HTTPS is off: a browser on another device gives a page its camera only over
  https. Turn it on with `[control.tls] enabled = true`, or pass `base`.
* The mixer listens only on this computer: set `[control] bind = "0.0.0.0:8080"`,
  or switch on **Let other devices on this network connect** in the desktop app.

`qr_svg` is the link as a QR code (byte mode, level M), an SVG document with
its quiet zone, for a page to show as it is.

### `guest.invite.list`

```json
{
  "guests": [
    { "id": "ana-lopez", "name": "Ana Lopez", "created": "...", "expires": "...",
      "state": "live", "source": "guest-ana-lopez", "bus": "guest-ana-lopez", "talkback": false }
  ],
  "talkback": { "source": "guest-talkback", "present": true, "held": [] },
  "max_guests": 8
}
```

| `state` | When |
|---|---|
| `invited` | Nobody has opened the link since the mixer started |
| `joining` | The guest's browser offered and the connection is being made |
| `live` | Their picture and sound are arriving |
| `gone` | They were live and left, or lost the network. The same link brings them back |

No key is ever in the list. `event/guest.changed` carries the same document
whenever any of it changes, to clients that subscribed to `guest.*`.

### `guest.invite.revoke`

The guest's camera session and their return end at once, and the source,
the bus and the return output are removed, in well under a second. A scene
that placed the source no longer shows it. The link lets nobody in
again: the page says the invite is gone.

### `guest.talkback.set`

| Param | What it is |
|---|---|
| `on` | Hold talkback (true) or let it go (false) |
| `guest` | The guest to talk to. Left out, every guest who has joined since the mixer started |
| `source` | Another input to talk with, such as a microphone on the mixer. From then on it is the talkback source |

The talkback source is `guest-talkback`, the operator's browser microphone,
unless `source` chose another. Whichever it is, it is muted on the programme
for as long as it is the talkback source, and a `source.audio.set` that
unmutes it leaves it muted. It reaches a guest's return only while talkback
is held there, at full level. When another source is chosen the old one is
an ordinary source again and stays muted until somebody unmutes it. Holding
is not saved: after a restart nobody is being talked to.

## Routes

On the control port, outside every token check, like `/whip/`:

| Route | Who | Answer |
|---|---|---|
| `POST /guest/{id}/whip` | the invite's key as `Authorization: Bearer` | `201`, the SDP answer, `Location` of the session |
| `POST /guest/talkback/whip` | a control token with operate | the same, for the operator's microphone |
| `POST /guest/{id}/whep` | the invite's key | `201`, the return's answer and its `Location` |
| `DELETE /guest/{id}/whip/{session}` | anyone with the session's name, which is random | `200` |
| `DELETE /guest/{id}/whep/{session}` | the invite's key | `200` |
| `PATCH` on either session | | `405`: every candidate is in the answer, and there is no ICE restart |

Under a station the routes are relayed to the show `?show=` names, and the
`Location` carries `?show=` back.

| Refusal | Status |
|---|---|
| No invite by that id | 404 |
| The wrong key, or an invite that has run out | 403 |
| Not SDP, or no `Content-Type: application/sdp` | 400 or 415 |
| Neither H.264 nor VP8 in the offer | 415 |
| Eight guests already joining or live | 503 |
| The return asked for before the guest's camera is in | 503, and the page tries again |
| GStreamer's WebRTC elements are missing | 501, naming the package to install |

A second offer for the same guest, from a page reloaded or a phone that
moved from Wi-Fi to cellular, ends the session they had and takes its place.

## What is made, and when

Nothing, while nobody is invited. An invite on its own is a line in a file.
On the guest's first offer, three things are added through the mixer's
queue, the way `source.add`, `audio.bus.add` and `output.add` add them:

| What | Id | It is |
|---|---|---|
| A source | `guest-<id>` | `guest://<id>`, named after the invite |
| A bus | `guest-<id>` | a mix minus of that source: the programme without them |
| An output | `guest-<id>-return` | a `whep/output` whose sound is that bus |

None of them is written to the runtime store, so a restart starts without
them and they come back when the guest does. They go when the invite is
revoked or runs out. While the guest is away and the invite still stands
they stay, holding the last picture for five seconds and black after that.

```text
 guest's browser --WHIP--> webrtcbin -> depay -> decode -> intervideosink / interaudiosink
                                                              | gmx-guest-<id>
 guest-<id> (guest/source)  intervideosrc / interaudiosrc  <--+
     -> the source's normaliser -> the programme, and every bus

 bus guest-<id> (minus guest-<id>) -> AAC -> whep/output guest-<id>-return
     programme video, as encoded -------^      -> Opus -> WebRTC -> the guest
```

The session pipeline is a pipeline of its own, built when the offer arrives
and taken down by a revoke or once the guest has gone: a `failed` or
`closed` connection at once, `disconnected` after fifteen seconds, and one
that never connects after twenty. The source is built once and reads
across the inter channel, so a guest's reconnect never rebuilds anything in
the mixer. H.264 is decoded with `avdec_h264` (or `openh264dec`), VP8 with
`vp8dec`; the picture keeps its size and is scaled by the source's
normaliser, and its rate is fixed at the canvas rate.

The return's picture is the programme encode, packetised and never encoded
again. Its sound is the guest's bus, made AAC by the bus's own encoder and
then Opus, the way any WHEP output on a bus is. The guest hears what is on
air, without themselves: a guest who is not on air is not in anyone's return
either, as in vMix.

## Limits and cost

Eight guests at once: a ninth offer is refused with 503 until one leaves.
Sixty-four invites a show.

Measured on an Apple M4 Pro (14 cores) on 29 October 2026, release build,
with `guest_cost`:

```sh
GMX_GUESTS=4 cargo test -p godwinmix-core --release --test guest_cost -- --ignored --nocapture guests_cost
```

| Guests live | The mixer, percent of one core | Above no guest | Each guest |
|---|---|---|---|
| none | 18.8 to 19.6 | | |
| 1 | 19.9 to 20.1 | 0.5 to 0.9 | 0.5 to 0.9 |
| 4 | 26.1 | 7.3 | 1.8 |
| 8 | 52.5 to 54.5 | 33.7 to 35.3 | 4.2 to 4.4 |

A 1280x720 canvas at 30 frames with one test source on air, each guest
sending 1280x720 H.264 at 30 frames and 2.5 Mbps with Opus, and watching
their return. Each run checks that every guest's own picture was decoded.
The machine was shared with other work (load average 4 to 6). The cost per
guest rises with the number of guests: one is under one percent of a core,
eight together are about a third of one. The guests' picture is a moving
test pattern, which is easier to decode than a real camera, so a real
guest costs somewhat more.

The guests' browsers run in child processes and are not counted. Most of a
guest is the decode of their 720p camera; the rest is the bus, its AAC
encoder, the Opus encoder of the return and two encrypted WebRTC sessions.
It has not been measured on a Raspberry Pi 4.

## Where they are kept

`<config name>.guests.json` beside the config: `godwinmix.guests.json` for
`godwinmix.toml`. It holds each invite's id, name, when it was made, when it
runs out, and its key, so the desk can show the link again. It is written
whole, through a temporary file and a rename, owner readable only on macOS
and Linux. Treat it like the config. A file that will not parse is moved
aside to `.json.unreadable` and the show starts with no invites.

## MCP tools

`create_guest_invite` and `list_guests`, behind `search_tools`.
