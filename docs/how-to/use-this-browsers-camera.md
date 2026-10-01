# Use this browser's camera and microphone

You have the mixer's page open in a browser, and the laptop it runs on has a
camera and a microphone. This puts them in the mixer as a source, with nothing
to install on the laptop and no second app. It takes about a minute.

It works today when the page is open on the mixer's own machine, at
`http://localhost:<port>` or `http://127.0.0.1:<port>`. From another computer
on the network it needs the page over https, which the mixer does not serve by
itself yet; see [When the browser says it cannot use a camera](#when-the-browser-says-it-cannot-use-a-camera).

## Before you start

The ingest plugin has to be installed, because the camera reaches the mixer
the way OBS would, by WHIP into a channel. If you can add a channel on the
Channels tab, it is there. If not, [Receive a phone or an OBS
stream](receive-a-phone-or-obs-stream.md#install-the-plugin) installs it.

WebRTC also needs GStreamer's libnice elements on the mixer's machine. Without
them the publisher stops with a sentence naming the package to install (on
macOS with Homebrew it is `libnice-gstreamer`).

## Open it

Press **Add a source** (Ctrl+N), then **Cameras**. The first rows are the
cameras plugged into the mixer; under them is **This browser's camera**. Press
**Open**.

**Microphones and audio** has **This browser's microphone** in the same place.
It opens the same card with the camera set to No camera. The palette
(Ctrl+K) has it as well, as Use this browser's camera and microphone.

A card opens in the bottom right corner of the page. It floats over everything
else and stays there while you change scenes, open drawers and close dialogs,
so the camera keeps going while you work. The browser asks once for the
camera and the microphone; allow both.

The card has:

* the picture, mirrored, the way a selfie camera shows you
* a level bar for the microphone
* **Camera** and **Microphone**, the devices this browser can see. No camera
  is a choice, for a microphone alone. The browser remembers what you picked,
  for next time
* **Echo cancellation, noise suppression and automatic gain**, on to start
  with. Switch it off for a music microphone or an instrument, which those
  three would flatten
* **Turn camera off** and **Mute microphone**
* **Go live**

## Go live

Press **Go live**. The line at the top goes from Connecting to Live, usually
within a second on the same machine. The card says
`In the mixer as browser-chrome-macos. Put it in a scene from Sources.`, with
your own browser and system in the name, and the source is in Sources like any
other. Put it in a scene, or take it.

Under the picture, once a second while the card is open, is what is going
out: the bit rate, the picture size and frame rate, and the round trip to the
mixer. The browser starts small and climbs towards 1280x720 at 30 frames a
second, about 2.5 Mbps, as it finds the bandwidth. On a slow link it keeps the
frame rate and gives up resolution, which is right for a person talking.

## While it is live

* **Another camera or microphone**: pick it in the list. It changes on the
  air, with no break in the stream.
* **Turn camera off**: the source shows black. **Mute microphone**: it goes
  silent. Neither one stops the stream, so the source stays where it is in
  every scene.
* **Fold away** (the `_` at the top) shrinks the card to its title. The
  picture, the level bar and the numbers stop while it is folded, and the
  camera keeps publishing. Press **Open** again in Add a source, or the
  palette entry, to bring it back.

## When the connection drops

The card says Reconnecting, with the reason, and publishes again by itself:
after a second, then two, four, eight, and every fifteen after that, until it
is back or you press **Stop**. The mixer side does the same thing it does for
any channel: a source that a scene holds shows its last picture until the
stream returns.

A refusal that waiting will not cure stops at once with the mixer's own
sentence: a key the channel does not have, a channel that is switched off or
does not take WHIP, or a browser with no H.264.

## Stop

Press **Stop**, or close the card with **×**. Closing a card that is live asks
first. Either way the stream ends, and the source leaves Sources unless a
scene holds it. Closing or reloading the page while it is live makes the
browser ask too.

## What it made

The first time, the button makes a channel called **Browser** (`browser`),
switched on, taking WHIP only, with each live stream made a source by itself.
It does that through `channel.add`, the same method the Channels tab and any
other client use, and reads the key back with `channel.key.reveal`. After
that it reuses the channel. If somebody has switched WHIP off on it, switched
the channel off, or turned off **Put each live stream in Sources**, it puts
those back with `channel.set` before it publishes.

The stream on that channel is named after the browser and the system,
`chrome-macos`, `firefox-windows`, `safari-ios`, so the source is
`browser-chrome-macos`. Two tabs in the same browser want the same name: the
second is turned away while the first is live, and keeps trying until the
first stops.

Nothing is decoded on the way in: the picture arrives as H.264, which every
current browser sends. The sound arrives as Opus and is turned into AAC, as
for any WHIP publisher on a channel.

## When the browser says it cannot use a camera

A browser gives a page the camera only at `https://` or at `localhost`. Opened
at `http://192.168.1.20:8080` from another laptop, the card says:

> This page is open at http://192.168.1.20:8080, which is not a secure
> address, so the browser will not let it use a camera or a microphone. Open
> the mixer's page at http://localhost on the mixer's own machine, or over
> https.

Until the mixer serves https itself, [put it behind a reverse proxy with
TLS](reverse-proxy.md) and open the page at the proxy's https address.

The other things the card can say, and what to do:

* **The browser was not allowed to use the camera.** It was refused once and
  remembered. Allow it in the site settings, at the left of the address bar.
* **The camera is in use by another program.** Close Zoom, Teams, Photo Booth
  or whatever else has it, then pick the camera again.
* **WHIP needs GStreamer's libnice elements.** Install the package it names on
  the mixer's machine, restart the mixer, and press **Go live** again.
* **Nothing is listening for channels.** The ingest plugin is not installed or
  is switched off. The card keeps trying, so it goes live once the plugin is
  on.

## The page on its own

The card is a page the mixer also serves by itself, at `/join/`. It takes the
address and the key in the link's fragment, which a browser never sends to a
server:

```
http://localhost:8080/join/#whip=/whip/browser/laptop&key=<the channel's key>&title=Laptop
```

That is how remote guests will join, once invites exist. They do not yet:
today the link carries the channel's own key, so give it only to someone you
would give the key to. [The publisher page](../reference/publisher-page.md)
lists what the link takes.

## Where to go next

* [Use a webcam](use-a-webcam.md), for a camera plugged into the mixer itself
* [Take streams from several encoders into one channel](channels.md)
* [Put it behind a reverse proxy with TLS](reverse-proxy.md)
