# Use transitions and effects from packs

Light leaks, bokeh, glitches, film burns, stingers, luma wipes and shader
transitions: the kind of pack people buy or download for Premiere, After
Effects, Resolve or OBS. This page gets one from a download folder onto the
programme, from the page, from the command line and from an AI agent.

What each file becomes and why is in
[the fx reference](../reference/fx.md).

## What you need

A mixer, and the pack's files: clips (WebM, MOV, MP4), still pictures for a
luma wipe, or `.glsl` shaders. The mixer reads a `.zip` as it is. Seven
starter items ship with it, so you can try everything below before you have a
pack: `light-leak`, `bokeh`, `glitch`, `film-burn`, `iris`, `ripple` and
`glitch-slice`.

If a pack only has project files for an editor (a MOGRT, an `.aep`, a
Resolve `.drfx`), render the transition out of that editor first, as ProRes
4444 or WebM with alpha for a stinger and as anything for a light leak.

## From the page

Studio mode puts the take bar between preview and programme.

1. Press **Looks** under the transition list. A panel opens with every
   imported transition, each moving in a small preview over a blue scene that
   becomes an orange one.
2. Press **Import**, or drop the files or the zip onto the panel. The mixer
   looks at each file, decides what it is and where it covers the picture,
   and adds it. A file it cannot use is listed with the reason.
3. Press **Use** on one. The transition list now shows it, and Take uses it.
4. Press **Default** to use it whenever a take names no transition, or
   **For** a scene to use it whenever that scene is taken. Cut is still a cut.

Effects get a row of buttons under the take bar. Each plays over whatever is
on air and goes when its clip ends; Alt+1 to Alt+9 press them from the
keyboard. On a Mac whose Option key types a symbol, give them other keys in
the shortcuts list.

On a phone the panel fills the bottom of the screen, two previews to a row,
and every button is big enough for a thumb.

## From the command line

```bash
gmx ctl rpc fx.import '{"path": "C:/Users/me/Downloads/light-leaks.zip"}'
gmx ctl rpc fx.list '{"role": "transition"}'
gmx ctl rpc program.take '{"scene": "wide", "transition": "light-leaks-04"}'
gmx ctl rpc fx.fire '{"name": "bokeh"}'
```

`fx.import` answers with what it made and what it skipped:

```json
{"imported": [{"name": "light-leaks-04", "kind": "overlay", "blend": "screen",
               "duration_ms": 2400, "cut_at_measured_ms": 1166, "coverage": 0.93,
               "transition": true, "effect": true, "origin": "library",
               "preview": "/api/v1/fx/light-leaks-04/preview.jpg", ...}],
 "skipped": [{"file": ".../readme.pdf", "reason": "..."}]}
```

A big pack answers with a `task_id` instead, because decoding twenty clips
takes longer than a call may hold you; `task.get` has the same answer when it
is done.

## From an AI agent

The tools are behind `search_tools`, so ask for them in words: "transition
effect light leak". Then:

```
import_fx {"path": "/home/me/Downloads/glitch-pack.zip"}
list_fx {"role": "effect"}
fire_fx {"name": "glitch"}
take {"scene": "Interview", "transition": "light-leak"}
assign_transition {"scene": "Interview", "transition": "light-leak"}
preview_fx {"name": "iris"}
```

`list_fx` says of each item whether `take` can use it (`transition`) and
whether `fire_fx` can play it (`effect`), so a small model does not have to
guess. A wrong name is answered with the names there are.

## When the cut shows

A stinger or a light leak cuts the scenes underneath at the frame where it
covers the most. If the old scene flashes before the cover, or the new one
shows too early, move the cut:

```bash
gmx ctl rpc fx.set '{"name": "light-leaks-04", "cut_at_ms": 1300}'
```

`"cut_at_ms": 0` puts back the frame the import measured. For one take only,
pass it in the take: `{"type": "light-leaks-04", "params": {"cut_at_ms": 1300}}`.

When `coverage` is under about 0.9 the clip never quite covers the picture,
and `fx.list` says so in its `note`. Such a clip is better fired as an effect
over a cut, or given `blend: add`, which covers more than Screen.

## When a light leak looks wrong

* It darkens the picture: it was imported as a stinger. Set `blend` to
  `screen`, or import again with `{"kind": "overlay"}`.
* Its black shows as a box: same cause, same fix.
* It is too strong as an effect: fire it with `{"opacity": 0.6}`.

## On a machine with no GPU

Clips and mattes always run on the CPU, and cost what the table in the
reference says. A shader runs on the GPU when GStreamer GL works there; on a
Raspberry Pi or a server with no display it runs its software version if it
has one (the two shipped shaders do) and as a dissolve if it does not.
`fx.list` says which in `runs`.

## See also

* [the fx reference](../reference/fx.md): formats, the folder, every method
* [change scene with a transition](transitions.md): the built in ones
* [transitions](../reference/transitions.md): how a transition lands on the frame
