# Invite a guest

A guest is somebody joining the show from their own phone or laptop, the way
a vMix Call or a Wirecast Rendezvous guest does. You send them a link. They
open it, allow the camera and the microphone, and press **Join the show**.
Their camera becomes a source you can put in any scene, and under their own
picture they see the programme and hear everyone on air but themselves, so
they never hear their own voice coming back late. You can talk to them off
air while you get them ready. Nothing to install at either end.

It takes about a minute. Up to eight guests can be on at once.

## Before you start

The guest's browser gives the page their camera only over https. The mixer
serves https on its own port ([Serve the mixer over https](serve-https.md)),
and the link uses the first address another device on the network can
open. If the mixer only listens on its own computer, or https is off, the
link dialog says so and what to change.

The mixer's https certificate is its own, signed by an authority the mixer
made itself, so a browser that has never met it warns before it opens the
page. Edge and Chrome say "Your connection isn't private", Safari says "This
Connection Is Not Private", Firefox says "Warning: Potential Security Risk
Ahead". Tell the guest to expect it. When the link is on an IP address or a
`.local` name, the invite dialog and the guest's own page each say so in one
line: choose Advanced, then continue. Which way round it depends on who they
are:

* **A guest on your own network, once.** They press **Advanced** and then
  **Continue to** the address (Safari: **Show Details**, then **visit this
  website**; Firefox: **Advanced**, then **Accept the Risk and Continue**).
  The browser remembers that for the address, so a reload does not ask again.
  Before they do, read them the fingerprint the mixer prints under its https
  line when it starts; the warning page shows the certificate behind
  **Advanced** or the padlock, and the two match.
* **Somebody who joins often, from their own phone or laptop.** Have them
  install the mixer's authority once, as [Trust this mixer on a
  phone](install-as-an-app.md#trust-this-mixer-on-a-phone) says. No warning
  after that, and it stays trusted when the mixer makes a new certificate.
* **A guest across the internet.** Give the mixer a name with a real
  certificate behind a reverse proxy ([Put it behind a reverse
  proxy](reverse-proxy.md)) and pass that address as `base`. No warning at
  all.

The warning is about who the page is from, not what it can do: past it the
page still asks for the camera and the microphone, and only gets them when
the guest allows it.

A guest on another network, across the internet, needs a way to reach the
mixer: a name and a port forward, a tunnel, or a reverse proxy
([Put it behind a reverse proxy](reverse-proxy.md)). Pass that address as
`base` and the link starts with it. The picture and sound then travel over
WebRTC, which finds its way with a public STUN server; two networks that
both block it need a TURN server, which this release does not set up for you.

## Invite them

1. Open **View > Guests**. On a phone it is under **More**.
2. Type the guest's name the way it should go on screen, `Ana Lopez`, and
   press **Invite**.
3. The dialog shows the link and a QR code. Press **Copy link** and send it to
   them by mail or a message, or let them scan the code if they are in the
   room with a phone.

The link works for a day. Anyone who has it can join as this guest, so send it
only to them. **Link** on their card shows it again.

The same thing from a terminal or an agent:

```sh
gmx ctl rpc guest.invite.create '{"name": "Ana Lopez"}'
```

An agent has `create_guest_invite` and `list_guests` behind `search_tools`.

## What the guest does

They open the link, past the certificate warning if they get one (see
[Before you start](#before-you-start)). The page is called **Join the show as
Ana Lopez**. They allow the camera and the microphone, pick others if they
like, and press
**Join the show**. Once they are live, **The show** appears under their own
picture, with the programme and its sound, within a second or two.
Headphones are best: the page cancels the echo of the show from their
microphone, but headphones mean there is none to cancel.

Until they pick one, the page opens a real camera rather than a program that
poses as one. A laptop with DroidCam, OBS or another virtual camera installed
often lists that first, and a browser asked for any camera takes the first.
If the browser's own choice will not open, because another program has it,
the page tries the other real cameras in turn. The list puts the real
cameras first. The guest can still pick the virtual one from it, and the
page keeps that choice next time.

If their connection drops, the page sends again by itself, and the show comes
back when it does. A reload or a new tab takes over from the old one.

## On the desk

Each guest has a card in **Guests**, with their picture once they are live,
and where they are:

| State | Means |
|---|---|
| Invited | The link has not been opened yet |
| Joining | Their browser is connecting |
| Live | Their camera and microphone are in the mixer |
| Gone | They left or lost the network. The same link brings them back |

* **Take** puts them on air full screen, as taking any source does. To put
  them in a scene beside other cameras, add their source, `guest-ana-lopez`,
  from **Sources** as you would any camera.
* **Mute** mutes their microphone on air and in everyone's return.
* **Talk** is press to talk: hold it, and your voice goes to that guest only,
  off air. **Talk to everyone** at the top does the same for every guest at
  once. The first press asks the browser for your microphone.
* **Link** shows the link and the code again.
* **Revoke** takes the invite back. Their camera and the show stop at once on
  their side, their source leaves the mixer, and the link stops working.

## What they hear

Each guest's return is a mix minus of the programme: what is on air, without
them. A guest who is not on air yet hears the show and nobody hears them. Two
guests on air hear each other and the host. While you hold **Talk**, your
microphone is added to their return only; it never goes on air, and it stays
muted on the programme even if somebody unmutes it there.

Talking from a microphone plugged into the mixer instead of from the browser:

```sh
gmx ctl rpc guest.talkback.set '{"on": true, "guest": "ana-lopez", "source": "producer-mic"}'
```

From then on `producer-mic` is the talkback source and is kept off air.

## When it does not work

* **The guest's page says the invite has run out, or that the key is wrong.**
  Make a new invite and send the new link.
* **The guest is stuck on Joining, or the show never appears under their
  picture.** The page reached the mixer, but the media cannot. WebRTC sends it
  over UDP, on ports the system picks. Allow inbound UDP for GodwinMix in the
  mixer machine's firewall (on macOS, allow it when the system asks), and keep
  the guest off a guest Wi-Fi that keeps devices apart.
* **The show under their picture stays black and silent, though the page says
  Live.** That is a mixer from 0.3.21 to 0.3.24: the return waited as long as
  the show had been running before it sent anything, so it only worked on a
  mixer started a moment before. Update the mixer.
* **This show already has its 8 guests on.** Revoke one who has finished.

## What it costs

A live guest costs the mixer the decode of their 720p camera, their mix minus
bus and the sound of their return; the picture they see is the programme's own
encode, sent again and never encoded twice. The figure for one machine is in
[the reference](../reference/guest-invites.md#limits-and-cost).

## Where to go next

* [The guest invite methods, routes and files](../reference/guest-invites.md)
* [Mix audio with buses](mix-audio-with-buses.md), for the mix minus underneath
* [Use this browser's camera](use-this-browsers-camera.md), for a camera in the room
