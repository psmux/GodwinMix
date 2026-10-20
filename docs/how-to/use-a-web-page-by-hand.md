# Use a web page source by hand

A web page source is a real Chromium the mixer draws off screen. Sometimes the
page needs a person: a play button, a cookie banner, a search, a video further
along, a quieter volume. **Interact** lets you do that from the desk, on the
picture that goes to air.

## Open Interact

In the Sources panel, a web page drawn by the browser renderer has a pointer
button beside its gear. Press it. The same thing is in the tile's menu (right
click, or a long press on a phone) as **Interact with the page**, and in the
source's settings drawer.

The dialog shows the page live, at the size it goes to air, with an address
bar over it. Press **Done**,
the cross, or Escape to leave. The live picture stops when the dialog closes,
and nothing about the page costs anything while nobody has it open.

## Click, type and scroll

* **Click** on the picture. A double click is a double click, and the right
  button is the right button.
* **Drag**: hold the button down and move, the way you would move a seek bar
  or a volume slider.
* **Scroll** with the wheel or the touchpad over the picture.
* **Type** once you have clicked the page: the keys go to whatever the page
  has focused, so a search box, a form, or a player's own keys (K to pause a
  YouTube video, the arrows for its volume and position).
* **On a phone**, a tap clicks and a drag scrolls the page. Press
  **Keyboard** to bring up the phone's keyboard; what you type goes to the
  page. An input method (Japanese, Chinese, Korean) sends its text when you
  confirm it.

Whatever you do is what goes to air. Pause a YouTube video and the programme
pauses; turn its volume down or mute it, and the sound the source sends to the
mixer goes down or stops with it. The source's own fader and mute in the mixer
are still there on top.

Once you have touched a page, the mixer stops pressing "unmute" on it until it
loads again. Before that it does, for a minute after each load, because a page
left on its own usually starts muted and waits for a click nobody makes.

## Go somewhere else

Type an address in the bar and press Enter or **Go**. `example.com` is enough;
the mixer adds `https://`. Only http and https addresses are taken.

The new address becomes the source's own: the mixer saves it, a restart opens
it, and a saved project keeps it. So does a link you followed on the page, so
the source comes back where you left it.

**←**, **→** and **⟳** are Back, Forward and Reload. Back and Forward are
greyed out when there is nowhere to go.

## YouTube

A YouTube address works as you would paste it: a `watch?v=` page, a
`youtu.be` short link, or an `/embed/` player. An embedded player is told to
start on its own and is sent the referrer YouTube asks an app for, so it no
longer stops at "Error 153".

Some YouTube videos still will not play, and the page says "Your browser
can't play this video". That happens when YouTube offers the video only in
H.264 or its sound only in AAC, which is common for live streams. On Windows
and Linux the browser inside the mixer plays both, so this does not happen
there. On macOS it carries no H.264 and no AAC decoder yet (VP9, AV1 and Opus
all play). When the renderer sees that error, the Interact dialog
says so in a line above the picture, and the source's status has it under
`page.notice`. A recording of the same stream, once YouTube has processed it,
usually plays. See [web page sources](../reference/web-page-sources.md#codecs)
for which builds have the codecs.

## A video in HEVC

HEVC (H.265) plays only with hardware video decoding, which is off for a web
page source unless you turn it on. When a page's video will not play and
HEVC is the likely reason, the Interact dialog says so and names the setting.
Open the source's settings, tick Hardware video decoding and Apply. The page
reloads in a second or so; if the source is on programme, its last picture
stays up until the page is back.
From the CLI:

```sh
gmx ctl rpc source.set '{"id": "promo", "params": {"gpu": true}}'
```

It needs a GPU that decodes HEVC; on a machine without one the notice says so.

## From the API, the CLI or an agent

The dialog uses two methods any client can call:

```sh
# A click at the middle of the page, then a key.
curl -X POST localhost:8080/api/v1/sources/youtube/page/input \
  -H 'authorization: Bearer <token>' -H 'content-type: application/json' \
  -d '{"events":[{"type":"down","x":0.5,"y":0.5},{"type":"up","x":0.5,"y":0.5},{"type":"key","code":75,"key":"k"}]}'

# Another page, kept as the source's address.
curl -X POST localhost:8080/api/v1/sources/youtube/page/navigate \
  -H 'authorization: Bearer <token>' -H 'content-type: application/json' \
  -d '{"action":"go","url":"example.com"}'
```

Points are shares of the page from 0.0 to 1.0, so a client never needs to know
the size the page is drawn at. An agent has the same two as the tools
`use_web_page` and `navigate_web_page`. Both are described in
[web page sources](../reference/web-page-sources.md#driving-a-page-by-hand).

## When it does not work

* **No pointer button on the tile.** The page is not drawn by the browser
  renderer: it is a transparent overlay, a superimposed page, or a mixer that
  fell back to GStreamer's `wpesrc`. Those take no clicks.
* **No live picture in the dialog.** The source is sending no picture, or it
  is restarting. The dialog asks again every two seconds while it is open.
  Clicks and keys still reach the page. The picture is the source itself
  (`/mjpeg/{source}?full=1`), not its cell of the multiview, so it does not
  need the multiview switched on.
* **"is starting again and cannot take events this moment".** The page's
  renderer is being restarted. Wait a second and try again.
