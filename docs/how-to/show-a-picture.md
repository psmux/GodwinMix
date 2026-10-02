# Show a picture, or a run of pictures

A holding slide, a sponsor's logo full screen, a title card, a set of
announcement slides exported as images: each is a picture source.

## One picture

1. Upload the picture in the **Media** tab, or drop it on the window.
2. Press **Add sources**, choose **Video and images**, and pick it.

PNG, JPEG, BMP, WebP, TIFF and SVG work, from this machine or from an
`https://` address. The picture is decoded once and held on screen for as long
as the source exists; it never runs out the way a clip does. Its sound is
silence.

## A logo, or anything with a transparent background

A PNG or WebP with an alpha channel, and any SVG, keeps its transparency: put
it in a scene over a camera and the camera shows through the clear parts. The
mixer looks at the file when the source is added and does this by itself;
there is nothing to switch on.

An SVG is drawn at the size it is placed at, not at the size it declares, so
it is sharp however large it is made. Resized in the composer, it is drawn
again at the new size once the item stops moving. A PNG is scaled once to its
new size, never every frame.

Two things to know:

* A transparent picture is drawn over every opaque item in the scene,
  whatever its place in the stack. Put a logo above the cameras, which is
  where a logo goes anyway. Among transparent items the stack order holds,
  and a camera with a chroma key on it is one of them: a desk PNG above a
  keyed presenter is drawn in front of the presenter. See
  [put a presenter in a virtual set](virtual-set.md).
* A picture behind an `https://` address is drawn flat unless it is an SVG,
  because the mixer does not fetch it to look before it decodes it. Upload it
  to the Media tab instead, or add the source with `alpha = true` in its params.

`alpha = false` draws a transparent picture flat, through the compositor, the
way every picture was drawn before.

## A clip with a transparent background

A stinger, an animated lower third, a sparkle over the picture: a clip with an
alpha channel plays over the scene the same way. WebM with VP8 or VP9 alpha,
ProRes 4444, QuickTime Animation and PNG frames in a MOV all keep their alpha.
Upload it, add it from **Video and images**, and it plays over whatever is
under it, looping as a clip does.

HEVC with alpha does not keep it on this build: it plays with its clear parts
filled in. Export it as ProRes 4444 or WebM VP9 instead. The full list, and
the decoder each format goes through, is in the [text and transparent sources
reference](../reference/text-sources.md#formats-that-keep-their-alpha).

Do not press **Convert** on a transparent clip in the Media tab. The converted
copy is H.264, which has no alpha, and the picker offers the converted copy
when there is one.

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
