# Capture SDI or HDMI from a DeckLink card

Broadcast cameras, a vision mixer's aux output or a playback machine usually
arrive on SDI. A Blackmagic DeckLink card (or an UltraStudio box) turns that
into something the mixer can take.

This has been built and checked without a card, not with one. The steps below
are what should happen; if they do not on your card, the source's health says
which part is missing, and that is worth reporting.

## Before you start

1. Install Blackmagic Desktop Video, the card's driver, and check in Desktop
   Video Setup that the card is there and its inputs see a signal.
2. In the page, open **Plugins** and add `decklink`.

## Add an input

1. Press **Add sources** and choose **Cameras**. Every DeckLink input the
   driver reports is listed by name.
2. Pick the one with your signal on it.

The connector (SDI or HDMI) and the video mode are detected. If the picture
does not come, open the source's settings, **Advanced**, and set them: the
connector the cable is in, and the mode the camera sends, such as `1080i50`.
The sound embedded in the signal comes with it unless you switch that off.

## When nothing is listed

The plugin's health, in **Plugins**, says which of three things is missing:

* **No DeckLink elements**: this GStreamer lacks gst-plugins-bad's decklink
  plugin.
* **No input is visible**: the Desktop Video driver is not installed, or does
  not see the card.
* An input opens but stays black: another program has it, or the connector
  or mode is wrong.

Every setting is in the [decklink plugin's README](../../plugins/decklink/README.md)
and its settings form.
