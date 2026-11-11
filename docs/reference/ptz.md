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
    fn second(&self) -> Option<Box<dyn Ptz>>;
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

`second` gives another driver on the same camera that may be used from
another thread while this one is busy. ONVIF, Canon and the fake give one;
VISCA gives `None`, because the camera keeps one sequence count for VISCA
over IP and many cameras take one TCP connection only. The fake has
`Head::save_delay` and `Head::query_delay` to stand in for a slow camera.

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
| `camera_add` | `id`, `protocol`, `address`, `user`, `password`, `profile`, `source`, `bus`, `optics` | adds a camera, or replaces one added before |
| `camera_list` | | every camera with its capabilities, speed, last error, tracking, optics and `lanes` |
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
| `track_tune` | `camera` and any of the numbers below | how it follows, read or changed live, and kept for the next start |

### Tuning how a camera follows

`track_tune {camera}` answers with the numbers as they stand. Give any of
them to change it; a value outside its range changes nothing and the error
says the range. A running follow takes the change on its next picture, and
`track_state` carries the tuning too.

| Number | Range | Default | What it does |
|---|---|---|---|
| `headroom` | 0 to 0.4 | 0.1 | space above the head, a fraction of the shot height |
| `lead_room` | 0 to 2 | 0.5 | how far ahead of a walking person to aim, in seconds of their walk |
| `dead_zone` | 0 to 0.3 | 0.06 | how far off centre the person may drift before the camera moves |
| `max_speed` | 0.05 to 1 | 0.8 | the fastest it turns or zooms, 1 being the camera's top speed. A ceiling: lowered while it moves, it holds at once |
| `max_accel` | 0.2 to 10 | 2.5 | how fast the speed may change each second; lower is smoother |
| `lost_hold_ms` | 0 to 30000 | 2000 | how long to hold the shot when the person vanishes |
| `switch_margin` | 1 to 5 | 1.5 | how much better someone else must score before it switches to them |
| `detect_every` | 1 to 10 | 1 | look for people on one picture in this many, tracking in between |
| `fallback` | `stay`, `wide`, `preset:<n>` | `stay` | what to do once the hold runs out |

A slower, smoother camera for a speaker who paces:

```json
{"name": "ptz/track_tune", "arguments": {"camera": "cam-pulpit", "max_speed": 0.4, "max_accel": 1.2, "headroom": 0.15}}
```

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
  "error": null,
  "camera_motion": {"dx": -0.18, "dy": 0.0, "zoom": 0.0},
  "detector": {"running": "yolox", "wanted": "yolox", "stage": "ready", "why": "yolox as asked for"}
}
```

`camera_motion` is how the camera's own movement moved the picture on the
last frame, in frames a second (see "Camera motion" below). `detector` says
which detector runs: `running` is `motion`, `yolox` or `yolox+pose`,
`wanted` is the setting, `stage` is `loading`, `ready` or `failed`, and
`why` says why they differ, for example "motion stands in while yolox
downloads into ... (first use only) and loads", or "motion stands in because
yolox could not be loaded: ...".

`mode` is `following`, `observing` or `stopped`. `state` is the engine's:
`idle`, `following`, `holding` (the person vanished, the shot holds), or
`fallback-stay`, `fallback-wide`, `fallback-preset` once the hold is over. On
`fallback-preset` with a preset named `preset-<n>`, the plugin moves the
camera there once and starts the tracker afresh.

### The joystick

`move` sets a speed that lasts `joystick_hold_ms` (300). Each call renews it;
when calls stop, the camera's own thread stops the camera. A panel sends it
every 100 ms while the stick is held. While it is held, the follow loop's
speeds are ignored: the operator always wins. `stop` ends the hold too.
`camera_list` counts the stops the watchdog made as `watchdog_stops` in the
worker status.

### Calls at the same time

Tool calls are answered concurrently. `move`, `camera_list`, `track_state`
and the other tools that only read the plugin's own state answer at once;
the tools that wait on a camera or the network (`stop`, `zoom`, `focus`,
`exposure`, `position`, `goto`, `preset_save`, `preset_recall`,
`track_observe`, `camera_add`) wait on a thread of their own, so one never
holds up another.

Each camera has two lanes when its protocol gives a second driver
(`lanes: 2` in `camera_list`: ONVIF, Canon, fake). The motion lane carries
`move`, the follow loop's speeds, `stop` and `zoom`; the command lane
carries the rest. A `preset_save` the camera takes two seconds over never
holds a move back, and the joystick keeps the camera moving through it.
VISCA cameras have one lane (`lanes: 1`): there a speed goes before any
queued command, but waits for the one command already on the wire, at most
the driver's timeout. If the joystick's hold runs out during that command,
the camera is stopped the moment it ends.

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

### The detector

`detector` picks how the follow loop finds people:

| Value | What runs | Cost a picture, one laptop core |
|---|---|---|
| `yolox` (default) | YOLOX nano at 320 by 256 | about 7 ms |
| `yolox+pose` | YOLOX nano, and MoveNet Lightning keypoints on the best person every third picture | about 12 ms on average |
| `motion` | the motion detector: no model, finds what moves | about 0.05 ms |

A loop starts on the motion detector and loads the model on a thread of its
own, fetching it into `models_dir` on first use (checked by SHA256), and
moves over to it once it is loaded. When the model cannot be had (offline on
first use, a damaged file, a build without the `yolox` feature), motion
stays and `track_state`'s `detector.why` says why. The motion detector loses
a person who stands still and sees anything that moves; it suits a
Raspberry Pi and a fixed wide shot.

The plugin's `yolox` cargo feature (on by default) brings in the model
detector; `cargo build -p gmx-ptz --no-default-features` builds a plugin
with the motion detector only.

### Camera motion

A camera that pans moves everything in its picture the other way. The
follow loop tells the tracking engine what speed the camera is moving at,
from the motion lane (the follow loop's own speed, or the joystick's while
it is held), and the camera's `optics` turn that into how fast the picture
moves. The tracker carries every box with the picture, so a pan is not read
as everyone walking and ids hold, and the motion detector rests while the
picture moves faster than 0.02 frames a second, learning the new view once
the camera is still. A `goto` or a preset recall moves the camera without a
speed the loop knows; a recall turns tracking off, and a preset fallback
starts the tracker afresh.

`optics` per camera, every field optional:

| Field | Default | What it is |
|---|---|---|
| `hfov_deg`, `vfov_deg` | 30, 17 | the field of view, degrees, at the zoom the camera mostly tracks at |
| `pan_deg_s`, `tilt_deg_s` | 60, 30 | degrees a second at full pan and tilt speed |
| `zoom_rate` | 0.7 | the natural log of how much the picture grows in a second at full zoom speed |

The defaults suit a 20x camera at a mid zoom. Optics a quarter off still
take three quarters of the camera's motion out. The engine follows its own
zoom commands from where the follow started, so a camera zoomed in twice as
far sees a pan move its picture twice as fast.

### Settings

| Key | Default | What it is |
|---|---|---|
| `cameras` | `[]` | `{id, protocol, address, user, password, profile, source, bus, optics}` per camera |
| `framing` | `waist` | the framing a follow starts with |
| `track_width` | 320 | the picture width the follow loop reads, 160 to 640 |
| `joystick_hold_ms` | 300 | how long one `move` lasts, 100 to 2000 |
| `detector` | `yolox` | `yolox`, `yolox+pose` or `motion` |
| `models_dir` | `GODWINMIX_MODELS`, else `models` under `GODWINMIX_HOME`, else `~/.godwinmix/models` | where the detector's models are kept |
| `data_dir` | `ptz` under `GODWINMIX_HOME`, or `~/.godwinmix/ptz` | presets' names and pictures, and the cameras added by tool |

### On disk

`<data_dir>/cameras.json` holds the cameras added with `camera_add`,
passwords included. `<data_dir>/<camera>/presets.json` holds each preset's
name and time, and `preset-<n>.jpg` its picture. `<data_dir>/<camera>/tuning.json`
holds what `track_tune` set.
