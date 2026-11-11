# PTZ cameras

Two parts. The crate `godwinmix-ptz` has the drivers: VISCA over IP, raw
VISCA over TCP, ONVIF PTZ and Canon CR-N, behind one trait. The plugin
`ptz` is a `service` that drives them for the mixer, the camera window and
agents, and runs the follow loop on `godwinmix-track`. How to set one up is
in [Add a PTZ camera](../how-to/add-a-ptz-camera.md) and
[Let a camera follow the speaker](../how-to/auto-track-a-speaker.md).

## The driver trait

```rust
pub trait Ptz: Send {
    fn protocol(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn move_at(&mut self, v: Velocity) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn zoom(&mut self, speed: f32) -> Result<()>;
    fn focus(&mut self, f: Focus) -> Result<()>;
    fn goto_preset(&mut self, n: u16) -> Result<()>;
    fn save_preset(&mut self, n: u16) -> Result<()>;
    fn position(&mut self) -> Result<Position>;
    fn goto(&mut self, p: Position) -> Result<()>;
    fn exposure(&mut self, e: Exposure) -> Result<()>;
}

pub fn open(target: &Target) -> Result<Box<dyn Ptz>>;
```

| Type | Fields |
|---|---|
| `Velocity` | `pan`, `tilt`, `zoom`, each -1 to 1. Right, up and tighter are positive. Below 0.01 is zero |
| `Position` | `pan`, `tilt` -1 to 1 across the camera's range, `zoom` 0 (widest) to 1, and `raw`, the protocol's own numbers |
| `Focus` | `Auto`, `Manual`, `OnePush`, `Drive(speed)` with -1 near to 1 far, 0 stops |
| `Exposure` | `mode` (`Auto`, `Manual`, `ShutterPriority`, `IrisPriority`), `iris`, `shutter`, `gain` in the camera's own steps; only what is set changes |
| `Capabilities` | `absolute`, `position`, `focus`, `exposure`, and `presets`, the highest preset number |
| `Target` | `protocol`, `address`, `user`, `password`, `profile` (ONVIF), `timeout` (800 ms) |
| `Protocol` | `visca-ip`, `visca-tcp`, `onvif`, `canon`, `fake` |

Presets are numbered from 1 everywhere: VISCA takes 1 to 256, ONVIF and
Canon 1 to 100. Every driver sends a speed it already sent only once, so a
loop that repeats the same speed ten times a second puts one command on the
wire.

### Errors

`Error { code, message }`. `code.slug()` is one of `unreachable`, `timeout`,
`refused`, `unauthorized`, `unsupported`, `bad-reply`, `bad-argument`. The
message says what happened and what to do next, for example: "the VISCA over
IP camera at 192.0.2.20:52381 did not answer. Check the address, that VISCA
over IP is on in its network menu, and that UDP port 52381 is not blocked."

### Fakes

`godwinmix_ptz::fake` has a `Head` that moves at the speed it was last given
and remembers presets, focus and exposure, and puts it behind each protocol
on loopback: `fake::visca::serve_ip`, `serve_tcp`, `fake::onvif::serve_onvif`
and `fake::canon::serve_canon`. `FakePtz` drives a head with no protocol and
is what the plugin's `fake` protocol uses.

## The plugin

Every tool answers a sentence in `content` and the facts in
`structured_content`. Through the mixer, call them with `tool.call` (`POST
/api/v1/tool/call`) as `ptz/<tool>`; the plan's `track/observe`,
`track/follow`, `track/stop` and `track/state` are `ptz/track_observe`,
`ptz/track_follow`, `ptz/track_stop` and `ptz/track_state`, because tool names
are lower case with underscores.

`camera` may be left out of any call when there is one camera.

| Tool | Arguments | Does |
|---|---|---|
| `camera_add` | `id`, `protocol`, `address`, `user`, `password`, `profile`, `source`, `bus` | adds a camera, or replaces one added before |
| `camera_list` | | every camera with its capabilities, speed, last error and tracking |
| `camera_remove` | `camera` | removes a camera added by tool; it is stopped first |
| `move` | `camera`, `pan`, `tilt`, `zoom` | the joystick: these speeds for 300 ms, renewed by each call |
| `stop` | `camera` | stops now and turns tracking off |
| `zoom` | `camera`, `speed` | zooms until a zoom with speed 0 |
| `focus` | `camera`, `mode`, `speed` | `auto`, `manual`, `one_push`, `near`, `far`, `stop` |
| `exposure` | `camera`, `mode`, `iris`, `shutter`, `gain` | `auto`, `manual`, `shutter`, `iris` |
| `position` | `camera` | where it points |
| `goto` | `camera`, `pan`, `tilt`, `zoom` | goes there; what is left out stays |
| `preset_save` | `camera`, `preset`, `name` | stores the shot and a 240 pixel thumbnail from the mixer |
| `preset_recall` | `camera`, `preset` | goes there and turns tracking off |
| `preset_list` | `camera`, `pictures` | names, times and thumbnails; `pictures: true` inlines them as data URLs |
| `track_follow` | `camera`, `framing`, `target`, `point` | tracking on. `target` is `person-<n>`, `largest` or `central`; `point` is `{x, y}` 0 to 1 from the top left |
| `track_stop` | `camera` | tracking off; the camera stays where it is |
| `track_framing` | `camera`, `framing` | `close`, `waist`, `full`, `wide` |
| `track_state` | `camera` | what the loop is doing; with no camera and several cameras, every camera |
| `track_observe` | `camera`, `wait_ms` | who is in view, without moving the camera |

### What the track tools answer

```json
{
  "camera": "cam-pulpit",
  "mode": "following",
  "state": "following",
  "framing": "waist",
  "target": "person-3",
  "tracks": [{"id": "person-3", "box": {"x": 0.58, "y": 0.22, "w": 0.09, "h": 0.6}, "confidence": 0.81, "state": "confirmed", "followed": true}],
  "shot": {"x": 0.44, "y": 0.12, "w": 0.36, "h": 0.5},
  "ptz": {"pan": 0.21, "tilt": 0.0, "zoom": 0.05},
  "frames": 412, "fps": 14.8, "step_ms": 0.9,
  "picture": {"width": 320, "height": 180, "from": "the mixer's MJPEG stream of 'pulpit'"},
  "age_ms": 40,
  "error": null
}
```

`mode` is `following`, `observing` or `stopped`. `state` is the engine's:
`idle`, `following`, `holding` (the person vanished, the shot holds), or
`fallback-stay`, `fallback-wide`, `fallback-preset` once the hold is over. On
`fallback-preset` with a preset named `preset-<n>`, the plugin moves the
camera there once and starts the tracker afresh.

### The joystick

`move` sets a speed that lasts `joystick_hold_ms` (300). Each call renews it;
when calls stop, the camera's own thread stops the camera. A panel sends it
every 100 ms while the stick is held. While it is held, the follow loop's
speeds are ignored: the operator always wins. `camera_list` counts the stops
the watchdog made as `watchdog_stops` in the worker status.

### Pictures for tracking

The follow loop reads the camera's picture only while tracking or observing
runs, at `track_width` (320) pixels across:

1. From the frame bus, when the camera has `bus` set (Linux and macOS): the
   decoded frame is read in place from shared memory and scaled down. No
   encode and no decode.
2. Otherwise from the mixer's `GET /mjpeg/<source>?width=320`, decoded here.
   The mixer makes that stream only while it has a reader.

An observing loop ends 15 seconds after the last `track_observe` or
`track_state` that asked. A following loop runs until `track_stop`, `stop`
or `preset_recall`.

### Settings

| Key | Default | What it is |
|---|---|---|
| `cameras` | `[]` | `{id, protocol, address, user, password, profile, source, bus}` per camera |
| `framing` | `waist` | the framing a follow starts with |
| `track_width` | 320 | the picture width the follow loop reads, 160 to 640 |
| `joystick_hold_ms` | 300 | how long one `move` lasts, 100 to 2000 |
| `data_dir` | `ptz` under `GODWINMIX_HOME`, or `~/.godwinmix/ptz` | presets' names and pictures, and the cameras added by tool |

### On disk

`<data_dir>/cameras.json` holds the cameras added with `camera_add`,
passwords included. `<data_dir>/<camera>/presets.json` holds each preset's
name and time, and `preset-<n>.jpg` its picture.
