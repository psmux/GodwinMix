# The camera and switcher controls' calls

Everything the page does with a PTZ camera or an ATEM switcher is a call any
client can make: a plugin tool through `tool.call`. The parts that send them
are camera properties (the side drawer a source tile's camera button
opens), the overlay on Studio mode's Preview monitor, the camera button and
badge on a source tile, the switcher chips in the Programme header, switcher
properties (the side drawer a chip or an Outputs row opens) and the **ATEM
switchers** section of the Outputs panel. The tool names live in one table,
`TOOLS` in `ui/panels/camera/api.js`, and the plugins' answers are turned
into what the page draws in `ui/panels/camera/shapes.js`.

`tool.call` hands back what the plugin answered, in MCP's shape with snake
case keys: `content`, a sentence for a person, and `structured_content`,
the facts. The page reads `structured_content`, and an answer with
`is_error: true` is shown as a refusal with its sentence.

Nothing here takes anything to air. A camera's source is put in Preview
the way any source is (a tap on its tile in Studio mode), and Take, Cut and
the transitions send it. The page sends no `program.take` of its own.

## When the page reads

One reader per page (`ui/panels/camera/hub.js` and `hub-read.js`) serves
every part, and reads only what a part on screen asked for:

| Asked for by | What is read | How often |
|---|---|---|
| any part | `plugin.list`, to learn whether `ptz` and `atem` are installed | every 30 seconds at most |
| the Sources panel on screen, Studio mode on, camera properties open | `ptz/camera_list`, then `ptz/track_state` per camera | every 5 seconds, and twice a second |
| camera properties open, Preview holding a camera | `ptz/track_observe` for that one camera | about eight times a second |
| the Programme monitor or the Outputs panel on screen, switcher properties open | `atem/list`, then `atem/state` per connected switcher | every 5 seconds, and twice a second |

With neither plugin installed and the demo off, only `plugin.list` is
asked. Nothing is read while the page is hidden.

## Cameras, through the `ptz` plugin

The tools are the `ptz` plugin's, named `ptz/<tool>`; its manifest is
`plugins/ptz/gmx-plugin.toml` and each tool's input schema is under
`plugins/ptz/schemas/tools/`.

| Tool | Arguments | What the page does with it |
|---|---|---|
| `ptz/camera_list` | | finds the camera whose `source` is a tile's source; reads `id`, `source`, `last_error` and `tracking` |
| `ptz/camera_add` | `id` (`cam-` and the source id), `protocol`, `address`, `source`, and `user` and `password` for ONVIF and Canon | **Add camera** in camera properties, for a source no camera points at yet |
| `ptz/move` | `camera`, `pan`, `tilt`, `zoom`, each -1 to 1 | the joystick and the zoom rocker, at most 15 calls a second and at least every 150 ms while held. Letting go, a lost window focus and a hidden page send all three at 0. The plugin holds each call for 300 ms and then stops the camera by itself |
| `ptz/focus` | `camera`, `mode` (`auto`, `manual`, `one_push`, `near`, `far`, `stop`), `speed` 0 to 1 | the focus switch, **Focus once**, and **Nearer** and **Farther** at speed 0.5 while held, then `stop` |
| `ptz/exposure` | `camera`, `mode` (`auto`, `manual`, `shutter`, `iris`), `iris`, `shutter`, `gain` | **Exposure and focus**; only what changed is sent |
| `ptz/preset_list` | `camera`, `pictures: true` | the saved shots, each with its picture as a data URL in `thumbnail_data` |
| `ptz/preset_save` | `camera`, `preset` (from 1), `name` | the **+** tile, at the next free number |
| `ptz/preset_recall` | `camera`, `preset` | a tap on a saved shot. Tracking, if it was on, goes off |

The page never sends `ptz/stop` or `ptz/zoom`. `stop` also turns tracking
off, which letting go of the joystick should not, and `zoom` keeps zooming
until told otherwise, which a dropped tab must not leave running.

## Tracking, through the `ptz` plugin

| Tool | Arguments | When |
|---|---|---|
| `ptz/track_follow` | `camera`, `target: "central"`, and `framing` when a framing chip turned it on | the Track switch, or a framing chip while tracking is off |
| `ptz/track_follow` | `camera`, `target: "person-N"` | a tap on a person's box, in camera properties or on Preview |
| `ptz/track_framing` | `camera`, `framing` (`close`, `waist`, `full`, `wide`) | a framing chip while tracking is on |
| `ptz/track_stop` | `camera` | the Track switch off |
| `ptz/track_observe` | `camera`, `wait_ms: 0` | for the boxes, see above |
| `ptz/track_state` | `camera` | for the words under the picture, the tile's badge and the switch |
| `ptz/track_tune` | `camera`, and any of `max_speed`, `max_accel`, `headroom`, `lead_room`, `dead_zone`, `lost_hold_ms`, `fallback` | **How it follows**: once with only `camera` when the section is opened, then at most ten times a second while a slider moves and once when it is let go |

Both track answers carry `mode` (`stopped`, `observing`, `following`),
`state` (`idle`, `following`, `holding`, `fallback-stay`, `fallback-wide`,
`fallback-preset`), `framing`, `target`, `error`, `tuning`, and `tracks`,
each `{id, box: {x, y, w, h}, confidence, state, followed}` with the box in
fractions of the picture from the top left. A track whose `state` is `lost`
is not drawn, and a box is cut where it leaves the picture. A person is
drawn as followed only while `mode` is `following`. The page says them
this way:

| The plugin says | Camera properties says | The tile's badge |
|---|---|---|
| `mode` not `following` | Tracking off | none |
| `mode: following`, `state: following` | Following: waist up (from `framing`) | Following |
| `mode: following`, `state` `holding` or a fallback | Lost them, holding the shot | Holding |
| `mode: following`, `state: idle` | Looking for someone | Looking |
| `error` set | No picture to track. Check the camera's source | No picture |

**How it follows** shows the `track_tune` numbers in words. Smoothness and
Steadiness are drawn so that right is smoother and steadier:

| Slider | Number | Right means |
|---|---|---|
| Speed | `max_speed` | faster |
| Smoothness | `max_accel` | lower, so smoother |
| Room above the head | `headroom` | more room |
| Lead room | `lead_room` | aims further ahead |
| Steadiness | `dead_zone` | a wider still zone |
| Hold when lost | `lost_hold_ms`, shown in seconds | holds longer |
| When lost | `fallback`: `stay`, `wide` or `preset:<n>` for a saved shot | |

A value the plugin refuses changes nothing, and its sentence is shown under
the sliders.

## Switchers, through the `atem` plugin

The tools are `plugins/atem`'s; [atem.md](atem.md) has every argument and
answer. The page sends:

| Tool | Arguments | From |
|---|---|---|
| `atem/list` | | the chips and the Outputs rows |
| `atem/state` | `switcher` | the chips, the rows and switcher properties |
| `atem/connect` | `id` (`atem-` and the name), `address`, `name`, `mode` | **Add switcher** |
| `atem/disconnect` | `switcher` | **Disconnect**, and **Remove** on an Outputs row |
| `atem/mode` | `switcher`, `mode` (`own`, `follow`, `lead`) | On its own, Follows GodwinMix, Leads GodwinMix |
| `atem/map` | `switcher`, `source`, `input` (null to unmap) | **Sources and inputs** |
| `atem/preview`, `atem/program` | `switcher`, `me: 0`, `input` | the Preview and Programme buses |
| `atem/cut`, `atem/auto` | `switcher` | Cut and Auto |
| `atem/transition` | `switcher`, `me: 0`, `style` or `rate` (frames) | **Transition** |
| `atem/dsk` | `switcher`, `keyer`, `action: "auto"` | DSK 1, DSK 2 |
| `atem/usk` | `switcher`, `me: 0`, `keyer`, `on` | Key 1, Key 2 |
| `atem/ftb` | `switcher` | FTB |
| `atem/aux` | `switcher`, `aux`, `input` | **Aux outputs** |
| `atem/macro` | `switcher`, `index`, `action: "run"` | **Macros** |

`atem/list` answers `{switchers: [{id, name, connected, mode, why}]}`, and
the chip and the row show `why` while a switcher is down. `atem/state`
answers the plugin's own shape (`inputs` with `long` and `short`, `mes`
with a `transition` each, `dsk`, `aux`, `macro`, `tally`, `map`); `atem()`
in `shapes.js` turns it into `inputs` (`id`, `name`, `short`), `mes`
(`program`, `preview`, `style`, `rate`, `position`), `dsks`, `usks`, `ftb`,
`auxes`, `macros` and `lastMacro`. The chip shows `short` for the inputs on
programme and preview.

A camera's picture in camera properties is the core's snapshot of the
GodwinMix source the camera is (`source` in `ptz/camera_list`), fetched
about three times a second while the drawer is open. The Preview overlay
draws only the boxes, over the Preview monitor's own picture.

## The demo

`ui/panels/camera/demo/` answers every call above in the browser, from a
pretend stage with three cameras and two switchers, with the same names
and shapes. **Try with simulated cameras**, in camera properties on a mixer
without the `ptz` plugin, turns it on: the three cameras stand in for the
source it was opened on and the next two video sources, the switchers map
them to inputs 1 to 3, and the switcher that follows GodwinMix moves with
GodwinMix's takes. While it is on, Preview shows the pretend camera's
picture under the boxes so the two agree. **Leave the demo** at the foot of
either drawer turns it off; this browser remembers which. The UI tests in
`ui/test/cameras.js` and `ui/test/cameras-parts.js` run against it.
