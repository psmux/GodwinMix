# Route inputs to outputs

The routing view is one screen with every input on the machine down the
side and every output across the top. Each cell says whether that input goes
to that output, in what format, where it is encoded, and what it costs. Open
it from View, Routing, or type "Routing" in the palette (`Ctrl+K`). The top
bar stays above it, so the show tabs and the live controls are still in reach.
`Escape` or the `×` closes it.

## What is on it

The inputs, in groups:

* each channel's streams, one row per stream that is publishing. A channel
  with nothing publishing has one row that says so.
* each show's programme, then that show's sources.

The outputs, in the same groups: each channel's destinations, then each
show's outputs. Every group of outputs ends in a `+` for adding one more.

A cell can hold:

| In the cell | Means |
|---|---|
| Copy, green | the stream goes out exactly as it arrives. No decode, no encode, free |
| Programme encode, grey | a show's output with no format of its own. It reads the programme's encoder, so it costs nothing extra |
| a format, blue | a rendition encoded on the GPU, such as `Facebook 720p30` over `GPU, VideoToolbox` |
| a format, amber | the same on the CPU, such as `1280×720 · 2.5 Mb/s` over `CPU, x264` |
| in programme | that source is on air in the show's programme, so this output carries it |
| a faint `+` | this destination could send this stream instead of the one it sends now |
| nothing | this input cannot reach this output |

When one encoder feeds several outputs the cell says `shared by 3`, and the
cost is that encoder's cost divided between them. Hover a cell for the whole
sentence. The format, the encoder and the cost come from `rendition.plan`,
the same plan the Outputs panel and the Resources tab read.

## Send a stream somewhere else

Click the faint `+` in an empty cell. A dialog says what that destination
sends now and asks for the format, with the same cards as Add destination:
Same as the source first and free, then the presets this machine can make,
each with its cost, then Custom. Choose one and press Send it. The
destination switches to that stream and reconnects.

If the machine has no room for the format you chose, nothing changes. The
dialog says what it would cost and offers a format that fits, as Add
destination does.

## Add an output

Press the `+` at the end of a group:

* for a channel, it opens the channel's destination form: pick the
  platform, paste the key, choose the format.
* for a show, it opens Add destination for that show, even when it is not the
  show the page is on.

A stopped show cannot take a new output. Its group says it is stopped and
has a Start button.

## Change a route

Click a filled cell. It opens that destination's own form, where its
format, its stream, its name and its key can be changed.

## Twenty inputs and twenty outputs

The screen is built to stay readable at that size:

* The output names stick to the top and the input names to the left, so
  both stay in view while the grid scrolls either way.
* Everything is grouped by channel and by show, and a route can only be
  inside its own group, so the routes sit in blocks down the diagonal and
  each block has a lighter background.
* Click a group's name to fold its inputs away.
* The filter box keeps what matches a name, a platform or a host. An output
  that matches is shown with every input it could read, and the other way
  round.
* On a phone the grid becomes a list, one card per output: what it reads,
  its format, where it is encoded and its cost, with a "Send backup instead"
  button for each other stream it could send.

## What it asks the mixer for

Nothing until it is opened. Then `channel.list` and `show.list` once, and
again when they change. Everything else is per group, and only for the
groups on screen: for a show, its `output.list`, `source.list` and
`rendition.plan` on a connection to that show; for a channel, its
`rendition.plan`. A group that has been scrolled out of view for three
seconds is let go, and closing the view lets go of everything.

## What it does not do yet

A channel's stream cannot be sent to a show's output from here. Add it to
the show as a source (Sources, Stream or feed), then put it on air.
