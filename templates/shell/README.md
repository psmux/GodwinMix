# {{name}}

A GodwinMix `{{kind}}` plugin in shell. It speaks JSON-RPC 2.0 on stdin and
stderr, one object per line, and writes media to stdout. No dependencies beyond
a shell and GStreamer's `gst-launch-1.0`.

## Try it

    gmx plugin test . --quick      # about fifteen seconds
    gmx plugin test . --offline    # no core at all, about a second
    gmx plugin add .
    gmx source add bars --type {{name}}/{{kind}}
    gmx ctl status

## What to change

`run.sh` has one `gst-launch-1.0` line that draws the picture. Replace it with
whatever produces yours: a capture device, an ffmpeg command, a renderer. As
long as it writes a container `decodebin` opens to stdout, nothing else has to
change.

`settings.json` is the only settings UI this plugin gets, and every surface
renders it: the web UI, the terminal UI and an agent all read the same schema.
Add a property with a `description` and some `examples` and it appears in all
three. `gmx plugin test` calls `configure` with every example you write, so
examples are worth having.

## The container transport costs a decode

The core demuxes what you write and decodes it if it was encoded. That is one
decode per source. If you can write raw frames to a socket instead, declare
`unixfd` in `transports` and the core pays nothing at all for the picture, on
Linux and macOS. `container` is what works everywhere and is the right first
choice.
