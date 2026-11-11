# The camera window's calls

Everything the Cameras window does is a call any client can make: a plugin
tool through `tool.call`, or a protocol method. The names live in one table,
`TOOLS` in `ui/panels/cameras/api.js`, so a tool renamed in the `ptz` or
`atem` plugin is one line to change there.

The `ptz` and `atem` plugins are being written alongside the window, and
the tool names below are the ones the window expects. Where a plugin ships
a different name, the table in `api.js` follows the plugin.

## Cameras, through the `ptz` plugin

| Tool | Arguments | What the window does with it |
|---|---|---|
| `ptz/camera_list` | | lists the cameras, every five seconds while on screen |
| `ptz/move` | `camera`, `pan`, `tilt` (each -1 to 1) | the joystick, at most 15 calls a second and at least every 150 ms while held; the plugin stops the camera 300 ms after the last one |
| `ptz/stop` | `camera` | when the joystick or zoom is let go, the window loses focus, or the page is hidden |
| `ptz/zoom` | `camera`, `speed` (-1 wider to 1 closer, 0 stops) | the zoom rocker, rate limited the same way |
| `ptz/focus` | `camera`, `auto` or `speed` | the exposure and focus drawer |
| `ptz/exposure` | `camera`, `mode`, `brightness` | the exposure and focus drawer |
| `ptz/preset_list` | `camera` | the saved shots, each with a `thumbnail` picture |
| `ptz/preset_save` | `camera`, `slot`, `name` | the **+** tile |
| `ptz/preset_recall` | `camera`, `slot` | a tap on a saved shot |
| `ptz/tracking` | `camera`, `on` | the Track switch |
| `ptz/framing` | `camera`, `style` (`close`, `waist`, `full`, `wide`) | the framing chips |

## Tracking

| Tool | Arguments | Answer the window reads |
|---|---|---|
| `track/observe` | `camera` | `people`: `{id, box: [x, y, w, h], score}` with the box in fractions of the picture, and `following`, the id followed. Read about eight times a second while on screen |
| `track/follow` | `camera`, and `person` or `point: [x, y]` | a tap on a person |
| `track/stop` | `camera` | |
| `track/state` | `camera` | `on`, `state` (`following`, `searching`, `manual`, `holding`, `lost`), `person`, `style`. Read twice a second |

## Switchers, through the `atem` plugin

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
| `program.key.set {scene: "Lower third", on}` | Lower third |

A camera's picture is the core's snapshot of the GodwinMix source the
camera is (`source` in `ptz/camera_list`), fetched about three times a
second while the window is on screen.

## The demo

`ui/panels/cameras/demo/` answers every call above in the browser, from a
pretend stage with three cameras and two switchers, with the same names
and shapes. The UI tests in `ui/test/cameras.js` run against it, and
`/test/cameras.html` shows it (`?theme=light`, `&pro=1`, `&follow=1`,
`&demo=0`).
