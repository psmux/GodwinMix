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
mixer. The source is named after the browser and the system, with four
letters of its own on the end, `browser-chrome-macos-k3f9` for example. The
browser keeps that name, so it comes back as the same source next time.

The last row under **Cameras** is **A phone's camera**, for a phone instead
of this computer; see [Add a phone's camera](#add-a-phones-camera).

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
name: `This browser's camera: live`. On a phone it sits just above the tabs.
Tap its name to open it out. Open, it has:

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
That includes the mixer taking the stream but refusing to make it a source,
which is the one case where the bar says live and still nothing can go in a
scene. The card then says why, for example that another program on the
mixer's computer already holds the RTMP port.
Fold it away and the same error, coming back on each retry, leaves it folded;
a different error opens it again.

Drag the bar by its name to put it anywhere else on the page. It stays there,
inside the window, and comes back there next time. Folded, it sits under any
dialog, so it never covers the button that finishes one. On a phone it goes
under a dialog open as well, since open it fills most of the screen. Left
where it starts on a phone, just above the tab bar, every screen scrolls far
enough to bring its last line out from under it, and Take outside Studio
mode floats above it.

Under the picture, once a second while the bar is open, is what is going
out: the bit rate, the picture size and frame rate, and the round trip to the
mixer. It sends 1280x720 at up to 30 frames a second, at most
2.5 Mbps. On a slow link it keeps the picture size and gives up frame rate,
because every change of size holds the source on its last picture for a
second or so while the mixer's decoder starts again at the new size.

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
any channel: a source that a scene holds shows its last picture for up to 45
seconds, then the slate, and the picture comes back by itself when the stream
returns.

A tab that was closed, a browser that was killed or a laptop that went to
sleep sends the mixer nothing on its way out, so its stream stays on the
channel for a moment. That moment is 2 seconds. A page reloaded, or a browser
started again, publishes to the same name and takes it over as soon as the old
stream has been silent that long, so it is back on air at once rather than
after WebRTC's own 30 second timeout. A second tab sending the same camera
while the first is still live is refused, as before.

A refusal that waiting will not cure stops at once with the mixer's own
sentence: a key the channel does not have, a channel that is switched off or
does not take WHIP, or a browser that can send neither H.264 nor VP8.

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

The stream on that channel is named after the browser and the system, with
four letters made up the first time and kept in the browser:
`chrome-macos-k3f9`, `safari-ios-m7qd`. So the source is
`browser-chrome-macos-k3f9`, and two laptops or two phones of the same kind
never ask for the same name. Two tabs in the same browser do: the second is
turned away while the first is live, and keeps trying until the first stops.

Nothing is decoded on the way in when the picture arrives as H.264, which
every current desktop browser sends. A browser that offers only VP8, which
some Android ones do, is taken too where the mixer has a VP8 decoder and an
H.264 encoder: its picture is decoded and encoded again as H.264, which costs
the mixer some CPU for as long as that phone is live. The sound arrives as
Opus and is turned into AAC, as for any WHIP publisher on a channel.

## Add a phone's camera

A phone on the same network as the mixer becomes a source with one scan, and
needs no app and no operator token. Each phone that scans the code is a
source of its own, so three phones are three cameras.

1. On the mixer's page, press **Add a source** (Ctrl+N), then **Cameras**,
   then **Show code** on **A phone's camera**. The first time this makes the
   **Browser** channel and installs the ingest plugin, as **Add** does above.
2. Point the phone's camera app at the code and open the link it finds. The
   link is the mixer's https address with `/join/` on the end, and the
   channel's key in the part after `#`.
3. The first time, the phone warns that the connection is not private. The
   certificate is the mixer's own, made for this machine. The dialog shows the
   first four pairs of its fingerprint; check them against the phone's
   certificate details, then go on to the page.
4. Allow the camera and the microphone. Type a name in **Name** if you like,
   such as `Ana's phone`; the line under it says the source it becomes,
   `browser-anas-phone`. Leave it empty and the phone keeps a made up name of
   its own, `browser-safari-ios-m7qd`.
5. Press **Go live**. The dialog on the mixer lists the phone as soon as it is
   live, and the source appears in Sources by itself.

On the phone:

* **Flip camera** switches between the front and the back camera. It is only
  there on a phone, or a computer, with more than one camera. The front
  camera's picture is mirrored on the phone, as a selfie camera is, and sent
  the right way round.
* Pick the **Shape** before you press **Go live**: **Landscape** sends
  1280x720, **Portrait** 720x1280 for Shorts, Reels and TikTok, and
  **Square** 720x720. The phone starts on the mixer's own shape, which the
  code carries, or on the one you picked last time for a mixer of that shape.
  The shape is held for as long as the phone is live: turn the phone, lock its
  rotation or flip the camera, and the mixer still gets the same size. Stop to
  change it.
* **Keep the picture upright when the phone turns** is on to begin with. It
  reads the phone's motion sensor, so a phone held on its side with auto
  rotate off still sends an upright picture. iPhones ask for permission to
  use motion the first time you press a button on the page; allow it. Turn
  the box off and the picture is turned only by **Rotate**.
* **Rotate** turns the picture a quarter turn clockwise each press, on top of
  whatever the sensor does. Use it for a phone in a holder at an odd angle.
* **Fill** crops the picture to fill the shape, so an upright phone sending
  Landscape gives a close up of the middle; the line under the buttons says
  so. **Fit** shows the whole picture with black bars. Both, and **Rotate**,
  work while live.
* The preview is exactly what is sent, mirrored for the front camera as a
  selfie camera is.
* The page keeps the screen on while it is live, where the browser allows it.
  A phone that locks or switches to another app stops its camera, and the
  source shows its last picture until the page is back in front. Keep the
  phone plugged in: the camera, the encoder and a screen that never sleeps
  drain a battery much faster than a call does.
* Firefox on Android has no way to keep the screen on. Set the phone's own
  screen timeout to its longest while it is a camera.

### Turn it or change how it fills, on the mixer

Right click a source's tile in **Sources** (press and hold on a touch
screen) while a scene is chosen above the tiles. **Rotate right** and
**Rotate left** turn it a quarter turn in that scene, **Upright again** puts it
back, and **Fill the box** or **Show all of it** chooses between cropping and
bars. They change that scene only, so the same camera can be upright in one
scene and on its side in another, and they apply on air at once. The
composer's inspector has the same and more, under **Fit** and the rotate
buttons.

### When the code is not shown

The dialog says why instead of showing a code a phone cannot use:

* **This mixer only listens on this computer.** A phone cannot reach a mixer
  bound to `127.0.0.1`. In the desktop app, switch on **Let other devices on
  this network connect** in Settings. On a server, set
  `[control] bind = "0.0.0.0:8080"` and restart the mixer.
* **HTTPS is off on this mixer.** A phone's browser gives a page its camera
  only over https. Turn it back on with `[control.tls] enabled = true` and
  restart ([Serve the mixer over https](serve-https.md)).

### When the phone says Reconnecting and never goes live

The page reached the mixer, but the picture cannot. WebRTC media goes to the
mixer over UDP, on one port per phone from 8189 up to 8204 (the ingest
plugin's `webrtc_port`). A firewall on the mixer's machine that lets the page
through on its TCP port but not UDP stops the picture. Allow inbound UDP 8189
to 8204 on the mixer's machine:

```sh
# Linux with ufw
sudo ufw allow 8189:8204/udp
```

```powershell
# Windows, from an administrator PowerShell
New-NetFirewallRule -DisplayName "GodwinMix WebRTC" -Direction Inbound -Protocol UDP -LocalPort 8189-8204 -Action Allow
```

On macOS with the firewall on, allow GodwinMix when the system asks. A guest
Wi-Fi that keeps devices apart from each other (client isolation) stops it
too; put the phone on the same network the mixer is on.

## When the browser says it cannot use a camera

A browser gives a page the camera only at `https://` or at `localhost`. Opened
at `http://192.168.1.20:8080` from another laptop, the bar opens and says:

> This page is open at http://192.168.1.20:8080, which is not a secure
> address, so the browser will not let it use a camera or a microphone. Open
> the mixer's page at http://localhost on the mixer's own machine, or over
> https.

Under it, **Open this page over https** opens the same address with
`https://` in front, on the same port. Accept the certificate warning once; [Serve the mixer over https](serve-https.md) says
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

The phone code is the same page with `channel=` in place of `whip=`, so each
phone picks its own stream name. That is how remote guests will join, once
invites exist. They do not yet:
today the link carries the channel's own key, so give it only to someone you
would give the key to. [The publisher page](../reference/publisher-page.md)
lists what the link takes.

## Where to go next

* [Use a webcam](use-a-webcam.md), for a camera plugged into the mixer itself
* [Take streams from several encoders into one channel](channels.md)
* [Serve the mixer over https](serve-https.md)
