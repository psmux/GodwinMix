# Preview monitor status

The programme panel shows "Preview live" while it receives mosaic frames. After two seconds without a frame it shows "Waiting for preview frames". When the panel releases its subscription because it is hidden, it shows "Preview paused while hidden".

This status describes the preview connection. It does not confirm that an external output is receiving video. Each output has its own status in the footer.

Nonseekable source tiles show "Continuous live source". Seekable sources retain their position slider. These labels do not change playback.

## Panel placement

The `monitor` slot holds the programme panel. Saved layouts that put it in `main` are migrated automatically. The built in control panels use collapsible sections in `main`; outputs and media previously placed in the footer move there as well.

The source audio and seek calls send the required `id` field. Source and output management controls follow the same id contract. `program.take` continues to use `source` or `scene`.

## Studio mode

Studio mode is the `producer` setting in this browser's `gmx.settings`, and it
is on unless somebody turned it off. Every setting used to be saved whenever
any one changed, so a stored `producer: false` is only kept when the same
object has `studioChosen: true`, which the Studio mode button, View > Studio
mode and Settings write. A browser that saved `false` before that marker
existed starts in Studio mode once, and keeps whatever is chosen after.

Its code (`panels/multiview/studio.js` and what it imports, and its
stylesheet) is fetched by an `import()` when the Programme panel first finds
the setting on, so a browser that turned it off never downloads it.

| What is in Preview | How it got there | What Take sends |
|---|---|---|
| a scene | `scene.preview.set {scene}` | `program.take {scene, transition}` |
| a source | kept by this page only | `program.take {source, transition}` |
| a suggested scene | nothing armed, or the armed scene is on air | `program.take {scene: <id>, transition}` |

Cut sends the same request with `transition: "cut"`. Take sends
`transition: {type, duration_ms}` with `params` when there is something in it:
`direction` for a wipe, slide or push, `colour` for a dip, and `easing` when it
is not the default. The choice is kept in this browser under
`gmx.studio.take`, and a count of takes per transition under
`gmx.studio.used`, which orders the three quick picks. The list is the built
in transitions plus what `program.transitions` adds, asked once, and the fx
library's transitions from `fx.list`, asked each time the picker opens.
**Default** and **For** a scene on an fx tile call `fx.assign`. An effect
calls `fx.fire {name}`; `fx.list {role: "effect"}` is asked once when Studio
mode loads, and its first nine are registered as `fx.fire-1` to `fx.fire-9`
on Alt+1 to Alt+9.

### The suggestion

With no scene armed, or the armed scene on air, and no source armed on this
page, Preview holds the scene most likely to be taken next: the newest scene
this page saw on air before the one on air now, else the first scene in the
list that is not on air. The page keeps that history from the status
document's `scene` field, so a take by any client moves it. It reads the scene
list through the same scene session the Scenes panel uses, and opens one when
that panel is not on the page.

A suggestion is not armed: nothing is called on the core to show it, and
`program.take {}` from another client still takes whatever is armed there. Its
picture is drawn on the page from the multiview mosaic the Programme monitor
already receives, at the boxes `scene.get` answers for it, as the Scenes
panel's pictures are, so the core composites nothing extra for it. Graphics
and text are not in that picture; an armed scene's preview stream has them.

Over every picture, a scene item whose source is not `live` in the status
document, has `has_video: false` or is not in the mixer is labelled on its own
box with the source's name and state. A source in Preview with no picture is
labelled over the whole picture.

The core's preview holds scenes only, so a source in Preview is this page's
own: it is drawn from the source's tile in the mosaic, and other pages do not
see it. Putting a source in Preview disarms the scene with
`scene.preview.set {}`, and arming a scene after that replaces the source, so
there is one thing in Preview.

A page loaded after a scene was armed reads the armed scene from `scene.list`,
since the status document has no field for it; `event/preview.changed` keeps it
current after that.

`program.take {source: ""}` is the slate even while a scene is armed. Cut to
black and `0` send that.
