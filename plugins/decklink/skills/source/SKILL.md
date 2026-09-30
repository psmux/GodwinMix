---
name: decklink-source
description: Capture an SDI or HDMI signal through a Blackmagic DeckLink card in GodwinMix. Use when the operator mentions SDI, a DeckLink, UltraStudio or Duo card, a broadcast camera on a BNC cable, or an HDMI capture card from Blackmagic.
---

# decklink/source

One input of a DeckLink card as a source, picture and embedded sound.

```
device.discover {}                       # every input the driver reports, as decklink/source candidates
source.add {id: "cam1", type: "decklink/source", uri: "decklink/source", device_number: 0}
source.add {id: "cam2", type: "decklink/source", uri: "decklink/source", device_number: 1, connection: "sdi", mode: "1080i50"}
```

## Things to know

* Three things must be there: GStreamer's decklink elements, Blackmagic's
  Desktop Video driver, and the card. The `decklink/devices` health says which
  is missing when discovery finds nothing.
* `mode` `auto` follows the signal. Set it only for a card that cannot detect.
* One input can be opened by one program at a time.
* Not tested on a real card: none was available where this was written.
