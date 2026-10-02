# Preview monitor status

The programme panel shows "Preview live" while it receives mosaic frames. After two seconds without a frame it shows "Waiting for preview frames". When the panel releases its subscription because it is hidden, it shows "Preview paused while hidden".

This status describes the preview connection. It does not confirm that an external output is receiving video. Each output has its own status in the footer.

Nonseekable source tiles show "Continuous live source". Seekable sources retain their position slider. These labels do not change playback.

## Panel placement

The `monitor` slot holds the programme panel. Saved layouts that put it in `main` are migrated automatically. The built in control panels use collapsible sections in `main`; outputs and media previously placed in the footer move there as well.

The source audio and seek calls send the required `id` field. Source and output management controls follow the same id contract. `program.take` continues to use `source` or `scene`.

## Studio mode

Loaded the first time Studio mode is switched on (`panels/multiview/studio.js`
and its stylesheet), so a page that never uses it does not download it.

| What is in Preview | How it got there | What Take sends |
|---|---|---|
| a scene | `scene.preview.set {scene}` | `program.take {scene, transition}` |
| a source | kept by this page only | `program.take {source, transition}` |

Cut sends the same request with no `transition`. Take sends
`transition: {type, duration_ms}` with the type and length chosen under the
button, and `params` when there is something in it: `direction` for a wipe,
slide or push, `colour` for a dip, and `easing` when it is not the default.
The choice is remembered in this browser. The list is the built in
transitions plus whatever `program.transitions` adds from the scene collection
and the plugins, asked once when Studio mode is first switched on.

The core's preview holds scenes only, so a source in Preview is this page's
own: it is drawn from the source's tile in the mosaic the Programme monitor
already receives, and other pages do not see it. Putting a source in Preview
disarms the scene with `scene.preview.set {}`, and arming a scene after that
replaces the source, so there is one thing in Preview.

A page loaded after a scene was armed reads the armed scene from `scene.list`,
since the status document has no field for it; `event/preview.changed` keeps it
current after that.

`program.take {source: ""}` is the slate even while a scene is armed. Cut to
black and `0` send that.
