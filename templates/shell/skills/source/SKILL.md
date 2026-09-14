---
name: {{name}}-source
description: {{description}} Use when an operator asks for this kind of picture in GodwinMix, or asks to add, configure or take a {{name}} source. Takes a bar count and a label; produces video only, at the canvas caps, over the container transport.
---

# {{name}} source

A GodwinMix source plugin. It produces video and no audio, at whatever canvas
the core is running, over the container transport (streamable Matroska on a
pipe), so it works on Linux, macOS and Windows alike.

## Add it

```
source.add {id: "{{name}}", type: "{{name}}/source", params: {bars: 8, label: "Test"}}
```

Wait for `event/source.state` to report `live` before taking it. Then:

```
program.take {source: "{{name}}"}
```

## Change it while it runs

```
source.set {id: "{{name}}", params: {bars: 4, label: "Half"}}
```

Every key in `schemas/source.json` can be set this way. The full object is sent,
not a diff, and it applies on the next frame with no restart and no gap.

## What it costs

Video only, drawn on demand, no decode in the core. The container transport
carries raw frames, so the pipe moves about 41 MB/s at 720p30 and 93 MB/s at
1080p30. Run `gmx plugin test .` to see the measured numbers on the machine you
are on.

## When it goes wrong

* `-32001 not live`: the source has not prerolled. Wait for `event/source.state`.
* The picture is grey or torn: `draw()` returned the wrong number of bytes. The
  plugin logs the two numbers and stops; read the log for the line.
* Nothing at all on the multiview: check `plugin.list` for the instance state.
  A `failed` instance has a reason in `detail`.
