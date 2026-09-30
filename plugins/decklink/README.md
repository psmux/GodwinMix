# decklink

SDI and HDMI capture through a Blackmagic DeckLink card (DeckLink Duo, Quad,
Mini Recorder, UltraStudio), with GStreamer's `decklink` elements.

| Provide | What it does |
|---|---|
| `decklink/source` | one card input, picture and embedded sound, the connector and video mode detected unless set |
| `decklink/devices` | every input the Blackmagic driver reports, as ready to add sources |

How to use it from the page is [docs/how-to/capture-sdi.md](../../docs/how-to/capture-sdi.md).

## What you need

* GStreamer's decklink elements, in gst-plugins-bad.
* Blackmagic Desktop Video, the card's driver, from blackmagicdesign.com.
* The card.

## Tested, and what is not

`cargo test -p gmx-decklink` checks the manifest, has GStreamer parse the
exact pipeline a card would get (with and without its sound), and checks that
a machine with the elements but no card lists no inputs and explains what to
check when one is opened. **No real card was available: the picture itself
has not been seen through this plugin.** Treat it as untested on hardware
until someone with a card has run it.
