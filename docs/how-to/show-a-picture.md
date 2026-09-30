# Show a picture, or a run of pictures

A holding slide, a sponsor's logo full screen, a title card, a set of
announcement slides exported as images: each is a picture source.

## One picture

1. Upload the picture in the **Media** tab, or drop it on the window.
2. Press **Add sources**, choose **Video and images**, and pick it.

PNG, JPEG, BMP, WebP and TIFF work, from this machine or from an `https://`
address. The picture is decoded once and held on screen for as long as the
source exists; it never runs out the way a clip does. Its sound is silence.

## A run of pictures

Export the slides with numbered names, `slide001.png`, `slide002.png` and so
on, and put them in one folder on the mixer. Add a source whose address is the
pattern, with the number written as `%03d` (three digits) or `%d` (any):

    /srv/slides/slide%03d.png

They play in order at 25 pictures a second and start again from the first.
For a slower run, set **fps** on the source: 1 shows each picture for a
second. The first number does not have to be 0 or 1; the source starts at the
lowest one it finds.

## When it does not show

* **"there is no picture numbered 0 to 9999"**: the pattern does not match the
  names. Check the number of digits: `%03d` finds `slide001.png`, not
  `slide1.png`.
* **A GIF plays as a clip.** Animated GIFs are opened as a clip, not as a
  still, so they move and end like one.

Every address the mixer takes is in the [sources reference](../reference/sources.md).
