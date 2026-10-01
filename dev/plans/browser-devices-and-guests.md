# Browser devices and remote guests

Two features that share most of their parts and are not the same thing.

1. **Browser devices.** The operator has GodwinMix open in a browser and wants
   the camera and microphone on that laptop as sources. No link, no invite.
   They press a button in the Sources drawer and pick the device.
2. **Remote guests.** The operator makes a link, sends it to someone, and that
   person joins from their own browser. Their camera and microphone arrive as
   sources. They see and hear the show while they are on it.

Both are a browser publishing over WebRTC to the mixer. The second adds an
invite, a waiting room, a return feed and a way for the operator and the guest
to talk to each other. So the plan builds the first, then grows the second out
of it.

## What already exists

More than it looks like from the UI.

| Piece | Where | What it gives us |
|---|---|---|
| WHIP into a channel, on the control port | `crates/godwinmix/src/control/whip.rs`, `plugins/ingest/src/whip.rs` | `POST /whip/<channel>/<stream>` with the channel key as bearer. A `webrtcbin` per session, no decode, media on a UDP range from `webrtc_port` (8189, 16 ports) only while a session is up |
| A channel stream becomes a mixer source | `crates/godwinmix/src/channels/auto.rs` | `auto_source`: the stream shows up in the mixer when it goes live, and leaves when it ends |
| `ingest/whip` source | `plugins/ingest` | A standalone WHIP endpoint on 8889 through `whipserversrc`. Takes any codec, decodes |
| WHEP playback | `crates/godwinmix/src/control/whep.rs` | `POST /whep/<output>`, a viewer key or read scope. This is the start of a return feed |
| Scoped tokens | `crates/godwinmix-protocol/src/scope.rs` | plugin, read, operate, admin. A guest scope fits here |
| The governor | `crates/godwinmix-govern` | Admission. A guest the machine cannot afford is refused before it costs anything |

What is missing is everything a person touches: a page that opens the camera
and publishes, a button in the UI, an invite link, a waiting room, a return
feed, and HTTPS.

## The thing that blocks both: a secure context

A browser only gives a page the camera and microphone in a secure context.
That is `https://` or `http://localhost`. The mixer speaks plain HTTP
(`docs/how-to/reverse-proxy.md` says so), so today:

* the operator on the same machine as the mixer, at `localhost:8080`, can use
  their camera
* the operator on another laptop at `http://192.168.1.20:8080` cannot. The
  browser hides the camera and says nothing useful
* a guest across the internet cannot, unless the operator has put a TLS proxy
  in front

This has to be solved before either feature is worth shipping. Options, in
the order I would take them:

1. **TLS on the control port, built in.** rustls is already in the build (the
   restreamer uses it). The mixer generates a local certificate on first run
   and serves HTTPS beside HTTP. On a LAN the browser warns once about the
   certificate; the operator accepts it once. Cheap and works offline.
2. **ACME (Let's Encrypt)** when the mixer has a public name. Needs port 80
   or a DNS challenge. Right for internet guests.
3. **The reverse proxy** we already document. Stays as the answer for people
   who already run Caddy or nginx.

The Tauri app is its own case: the webview is a secure context, but macOS
needs the camera and microphone entitlements in the app bundle.

## Media path

A browser device and a guest both publish by WHIP into a channel, and the
channel's `auto_source` puts them in the mixer. That reuses the listener, the
key check, the live events, the turn away messages and the port handling we
already have, and it means the third party API sees nothing a first party
client does not (rule 2).

* Browser devices go to a channel called `browser`, one stream per device:
  `browser/<operator>-cam`, `browser/<operator>-mic`.
* Guests go to a channel called `guests`, one stream per guest:
  `guests/<guest-slug>`.

The catch: channel WHIP takes H.264 only, because the channel path never
decodes. The guest page sets `setCodecPreferences` to H.264 first. Chrome,
Edge and Safari send H.264 on every platform we care about. Firefox does too
through OpenH264, which it downloads itself. A browser that offers only VP8 is
accepted too, and its picture is decoded in the ingest plugin and handed on.
That costs a decode we would pay at the mixer anyway, so it is not a loss for
a guest, only for a restreamed channel.

Audio is Opus from every browser, and the channel path turns it into AAC
today (`plugins/ingest/src/whip/session.rs`: `opusdec ! avenc_aac`), because
the channel hub carries what FLV can. For a channel that is restreamed that
is right. For a guest it is a decode, an encode and a few tens of
milliseconds of latency for nothing, since the mixer decodes it again
straight away. Guest and browser streams should hand Opus to the mixer as it
came. That is a change to the hub's tag type or a side door for these two
channels, and wave 0 decides which.

## Performance

The rule is the programme never stops, and every guest is a cost we can count.

| Cost | Where | How big |
|---|---|---|
| WebRTC receive, SRTP, jitter buffer | ingest plugin, one session per guest | Small. No decode |
| Decode the guest's picture | the mixer, once per guest on air | One H.264 decode. Hardware where there is one (v4l2 on the Pi, VideoToolbox, VAAPI) |
| Return picture | one shared encode of the programme at a low size | One encoder for every guest together, and only while a guest is connected |
| Return sound | one Opus encode per guest (mix minus) | Small. Opus at 48 kbps is cheap |
| Blur, noise suppression, echo cancellation | the guest's browser | Nothing on the mixer |

Guests in the waiting room are not decoded. Their preview is a low size
snapshot the browser sends, or the stream decoded only while the operator is
looking at it (rule 1, nothing runs unless asked).

The governor admits each guest and each return feed. On a Pi that probably
means two or three guests at 720p. Wave 0 measures it instead of guessing.

The guest page sets the encoder on the browser side: 1280x720 at 30, about
2.5 Mbps, `degradationPreference: maintain-framerate` for a person talking,
`maintain-resolution` for a screen share. The browser adapts to its own
uplink; the mixer does not have to.

## Modules

Each one small, each one its own place in the tree, each with its own tests
and docs.

| Module | Where | What it is |
|---|---|---|
| The publisher page | `ui/join/` | Static HTML and plain JS, no framework, no build step. Device picker, preview, level meter, WHIP client, reconnect, stats. Shared by both features |
| Publisher button | `ui/panels/sources/` | "This browser's camera" and "This browser's microphone" in the Sources drawer. Opens the publisher in a panel, not a new tab |
| Guest protocol | `crates/godwinmix-protocol/src/guests.rs` | Types, methods, events, errors, the `guest` scope. No media stack |
| Guest registry | `crates/godwinmix/src/guests/` | Invites, their secrets, expiry, the waiting room, admit and remove. Talks to channels through the same calls the UI uses |
| Guest routes | `crates/godwinmix/src/control/join.rs` | `GET /join/<invite>` serves the page, `GET /join/<invite>/ws` carries tally, mute requests and chat |
| Return feed | `crates/godwinmix-core/src/mixminus/` | A sound bus per guest that is the programme without that guest, and a shared low size programme picture. Exists only while a guest is connected |
| Guests panel | `ui/panels/guests/` | Invite, waiting room tiles, admit, mute, remove, names |
| TLS | `crates/godwinmix/src/tls/` | Local certificate, then ACME |

Methods, all on the one protocol:

```
guest.invite   {name?, expires_in?, can_share_screen?}  → {id, link}
guest.list     → [{id, name, state: invited|waiting|on|left, source?}]
guest.admit    {id, scene?}
guest.mute     {id, audio?, video?}    asks; the guest's browser does it
guest.remove   {id}                    ends the session, the link stops working
guest.revoke   {id}                    the link stops working, nobody was there
```

Events: `event/guest.changed`, `event/guest.removed`. Errors name the next
step, as everywhere else: an expired link says it expired and that the
operator can send another.

The invite secret goes in the link's fragment (`/join/sam-k#k=...`), so it
never lands in a proxy's access log. The guest page reads it and sends it as
the WHIP bearer. A guest token can publish to its own stream and play its own
return feed. Nothing else.

## Waves

Each wave ends building, tested, documented, committed and pushed.

**Wave 0. Measure.** About a day. Publish from Chrome, Safari and Firefox
into a channel over WHIP from a page served on localhost. Confirm H.264 arrives
untouched and the mixer shows it. Decide how Opus reaches the mixer without
the AAC detour. Measure CPU per guest on the Pi and on
an old laptop. Write the numbers down here, with glass to glass latency.

**Wave 1. Browser devices.** The publisher page, the Sources drawer buttons,
the `browser` channel made on first use. Works at `localhost` with no TLS.
Feature 1 is done for the operator on the mixer's own machine.

**Wave 2. TLS.** Local certificate on the control port. Feature 1 now works
from any laptop on the LAN.

**Wave 3. Invites.** The protocol, the registry, the `guest` scope, the
`/join` route, the link. A guest joins and appears as a source.

**Wave 4. Waiting room and Guests panel.** Guests wait until admitted. The
operator sees them, names them, admits them to a scene, mutes or removes
them. The guest page shows an on air lamp.

**Wave 5. Return feed.** The shared programme picture and the mix minus
sound per guest, so two guests can talk to each other and to the host without
an echo.

**Wave 6. Internet guests.** STUN and TURN settings (an external coturn
first), ICE over TCP for networks that block UDP, ACME. A how to page for
running a show with guests in other cities.

**Wave 7. The StreamYard part.** Screen share as a second source, chat, the
guest's name as a lower third, layouts that rearrange as guests come and go
(solo, side by side, grid). Background blur on the guest's side.

**Later.** Local recording in the guest's browser, uploaded after the show,
for a clean copy that never went through their uplink.

## Decisions

Made on 2026-10-01.

* TLS is built in, with a local certificate first and ACME later.
* Guests may send VP8 as well as H.264. H.264 stays the preferred codec on the
  guest page, because it needs no decode in the channel path; VP8 is accepted
  and decoded so that no browser is turned away.
* Built with parallel agents, each in its own git worktree, at most two at a
  time, with the cost stated before each launch.
* Still open: how many guests a Pi and an old laptop should carry. Wave 0
  measures it.
