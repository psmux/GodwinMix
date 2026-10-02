# Use this browser's camera and microphone

You have the mixer's page open in a browser, and the laptop it runs on has a
camera and a microphone. This puts them in the mixer as a source, with nothing
to install on the laptop and no second app. It takes about a minute.

It works when the page is open on the mixer's own machine, at
`http://localhost:<port>`, or from any other computer over https, which the
mixer serves on the same port ([Serve the mixer over https](serve-https.md)).
At a plain `http://` address from another computer the browser hides the
camera; see [When the browser says it cannot use a camera](#when-the-browser-says-it-cannot-use-a-camera).

## Add it

Press **Add a source** (Ctrl+N), then **Cameras**. The first rows are the
cameras plugged into the mixer. Under them are this browser's cameras, each
named with `(this browser)` after it, and an **Add** button, as the mixer's
own have. The first time, before the browser has been allowed a camera, there
is one row instead, **This browser's camera**.

Press **Add**. The browser asks once for the camera and the microphone; allow
both. It starts sending straight away, and as soon as the mixer has the
source it is put in the scene you were adding to, the same as a camera on the
mixer. The source is named after the browser and the system,
`browser-chrome-macos` for example.

**Microphones and audio** lists this browser's microphones the same way, for
sound with no picture. The palette (Ctrl+K) has it as well, as Use this
browser's camera and microphone.

If the mixer does not have the ingest plugin, which is what takes a
browser's stream in, the first **Add** installs it, and a note says so. That
can take a minute. WebRTC also needs GStreamer's libnice elements on the
mixer's machine; without them the bar opens and names the package to install
(on macOS with Homebrew it is `libnice-gstreamer`).

### The same camera twice

When the page is open on the mixer's own computer, its cameras are the
mixer's cameras too, and the picker lists each one twice: once as a camera on
the mixer, once as `(this browser)`. Add only one of them. A camera has one
picture size for everything that has it open, so when the browser opens it at
another size the mixer's camera source holds still, and its log says why.

## The bar

A browser's camera belongs to the tab that opened it, so a slim bar stays at
the bottom right of the page while it is sending, with the state after its
name: `This browser's camera: live`. Press **_** on it to open it out. Open,
it has:

* the picture, mirrored, the way a selfie camera shows you
* a level bar for the microphone
* **Camera** and **Microphone**, to switch device while it is live. No camera
  is a choice, for a microphone alone. The browser remembers what you picked,
  for next time
* **Echo cancellation, noise suppression and automatic gain**, on to start
  with. Switch it off for a music microphone or an instrument, which those
  three would flatten
* **Turn camera off** and **Mute microphone**
* **Stop**, and **Send to the mixer** to start again after a stop

It opens out by itself when something goes wrong, so the reason is on screen.

Under the picture, once a second while the bar is open, is what is going
out: the bit rate, the picture size and frame rate, and the round trip to the
mixer. It sends 1280x720 at up to 30 frames a second, at most
2.5 Mbps. On a slow link it keeps the picture size and gives up frame rate.

## While it is live

* **Another camera or microphone**: pick it in the list. It changes on the
  air, with no break in the stream.
* **Turn camera off**: the source shows black. **Mute microphone**: it goes
  silent. Neither one stops the stream, so the source stays where it is in
  every scene.
* **Fold away** (the `_` at the top) shrinks it back to the bar. The
  picture, the level bar and the numbers stop while it is folded, and the
  camera keeps sending.
* Adding another of this browser's devices from the picker switches the one
  stream to it, rather than starting a second.

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

Press **Stop**, or close the bar with **×**. Closing it while it is live asks
first. Either way the stream ends, and the source leaves Sources unless a
scene holds it. Closing or reloading the page while it is live makes the
browser ask too.

## What it made

The first time, **Add** makes a channel called **Browser** (`browser`),
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
at `http://192.168.1.20:8080` from another laptop, the bar opens and says:

> This page is open at http://192.168.1.20:8080, which is not a secure
> address, so the browser will not let it use a camera or a microphone. Open
> the mixer's page at http://localhost on the mixer's own machine, or over
> https.

Open the same address with `https://` in front instead, and accept the
certificate warning once; [Serve the mixer over https](serve-https.md) says
how to check it is the mixer's certificate.

The other things the bar can say, and what to do:

* **The browser was not allowed to use the camera.** It was refused once and
  remembered. Allow it in the site settings, at the left of the address bar.
* **The camera is in use by another program.** Close Zoom, Teams, Photo Booth
  or whatever else has it, then pick the camera again.
* **WHIP needs GStreamer's libnice elements.** Install the package it names on
  the mixer's machine, restart the mixer, and press **Send to the mixer**.
* **Nothing is listening for channels.** The ingest plugin was removed or
  switched off after the source was added. The bar keeps trying, so it goes
  live once the plugin is back.

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
* [Serve the mixer over https](serve-https.md)
