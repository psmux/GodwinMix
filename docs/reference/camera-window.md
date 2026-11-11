# The camera window's calls

Everything the Cameras window does is a call any client can make: a plugin
tool through `tool.call`, or a protocol method. The names live in one table,
`TOOLS` in `ui/panels/cameras/api.js`, and the plugin's answers are turned
into what the window draws in `ui/panels/cameras/shapes.js`.

`tool.call` hands back what the plugin answered, in MCP's shape with snake
case keys: `content`, a sentence for a person, and `structured_content`,
the facts. The window reads `structured_content`, and an answer with
`is_error: true` is shown as a refusal with its sentence.

## Cameras, through the `ptz` plugin

The tools are the `ptz` plugin's, named `ptz/<tool>`; its manifest is
`plugins/ptz/gmx-plugin.toml` and each tool's input schema is under
`plugins/ptz/schemas/tools/`.

| Tool | Arguments | What the window does with it |
|---|---|---|
| `ptz/camera_list` | | lists the cameras every five seconds while on screen; reads `id`, `source`, `last_error` and `tracking` |
| `ptz/move` | `camera`, `pan`, `tilt`, `zoom`, each -1 to 1 | the joystick and the zoom rocker, at most 15 calls a second and at least every 150 ms while held. Letting go sends all three at 0. The plugin holds each call for 300 ms and then stops the camera by itself |
| `ptz/focus` | `camera`, `mode` (`auto`, `manual`, `one_push`, `near`, `far`, `stop`), `speed` 0 to 1 | the focus switch, **Focus once**, and **Nearer** and **Farther** at speed 0.5 while held, then `stop` |
| `ptz/exposure` | `camera`, `mode` (`auto`, `manual`, `shutter`, `iris`), `iris`, `shutter`, `gain` | the exposure drawer; only what changed is sent |
| `ptz/preset_list` | `camera`, `pictures: true` | the saved shots, each with its picture as a data URL in `thumbnail_data` |
| `ptz/preset_save` | `camera`, `preset` (from 1), `name` | the **+** tile, at the next free number |
| `ptz/preset_recall` | `camera`, `preset` | a tap on a saved shot. Tracking, if it was on, goes off |

The window never sends `ptz/stop` or `ptz/zoom`. `stop` also turns tracking
off, which letting go of the joystick should not, and `zoom` keeps zooming
until told otherwise, which a dropped tab must not leave running.

## Tracking, through the `ptz` plugin

| Tool | Arguments | When |
|---|---|---|
| `ptz/track_follow` | `camera`, `target: "central"`, and `framing` when a framing chip turned it on | the Track switch, or a framing chip while tracking is off |
| `ptz/track_follow` | `camera`, `target: "person-N"` | a tap on a person's box |
| `ptz/track_framing` | `camera`, `framing` (`close`, `waist`, `full`, `wide`) | a framing chip while tracking is on |
| `ptz/track_stop` | `camera` | the Track switch off |
| `ptz/track_observe` | `camera`, `wait_ms: 0` | about eight times a second while on screen, for the boxes. A camera that refuses (no source yet) is asked again after five seconds |
| `ptz/track_state` | `camera` | twice a second, for the words under the picture |

Both track answers carry `mode` (`stopped`, `observing`, `following`),
`state` (`idle`, `following`, `holding`, `fallback-stay`, `fallback-wide`,
`fallback-preset`), `framing`, `target`, `error`, and `tracks`, each
`{id, box: {x, y, w, h}, confidence, state, followed}` with the box in
fractions of the picture from the top left. The window says them this way:

| The plugin says | The window says |
|---|---|
| `mode` not `following` | Tracking off |
| `mode: following`, `state: following` | Following: waist up (from `framing`) |
| `mode: following`, `state` `holding` or a fallback | Lost them, holding the shot |
| `mode: following`, `state: idle` | Looking for someone |
| `error` set | No picture to track. Check the camera's source |

## Switchers, through the `atem` plugin

The `atem` plugin is still being built. These are the names the window
sends for now, and the table in `api.js` will follow the plugin.

| Tool | Arguments |
|---|---|
| `atem/list` | |
| `atem/state` | `switcher` |
| `atem/cut`, `atem/auto` | `switcher`, `me` |
| `atem/preview`, `atem/program` | `switcher`, `me`, `input` |
| `atem/dsk` | `switcher`, `keyer`, `action` (`on`, `off`, `auto`) |
| `atem/usk` | `switcher`, `me`, `keyer`, `on` |
| `atem/ftb` | `switcher` |
| `atem/aux` | `switcher`, `aux`, `input` |
| `atem/macro` | `switcher`, `index`, `action: "run"` |
| `atem/transition` | `switcher`, `me`, `style`, `rate` (frames) |
| `atem/mode` | `switcher`, `mode` (`own`, `follow`, `lead`) |

The window reads from `atem/state`: `name`, `connected`, `mode`, `inputs`
(`id`, `name`, `short`), `map` (GodwinMix source id to ATEM input), and
`mes`, `dsks`, `usks`, `ftb`, `auxes` and `macros`.

## GodwinMix's own programme

| Method | When |
|---|---|
| `program.take {source}` | Cut |
| `program.take {source, transition: "fade"}` | Fade |
| `program.take {source: ""}` | Black |
| `program.key.set {scene, on}` | Lower third. The scene is `Lower third` unless Pro view names another |

A camera's picture is the core's snapshot of the GodwinMix source the
camera is (`source` in `ptz/camera_list`), fetched about three times a
second while the window is on screen.

## The demo

`ui/panels/cameras/demo/` answers every call above in the browser, from a
pretend stage with three cameras and two switchers, with the same names
and shapes. The UI tests in `ui/test/cameras.js` run against it, and
`/test/cameras.html` shows it (`?theme=light`, `&pro=1`, `&follow=1`,
`&demo=0`).
