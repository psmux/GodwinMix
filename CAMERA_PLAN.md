# Camera control, tracking and ATEM: the plan

A church, a school hall or a small studio usually has two to four PTZ cameras,
one or two Blackmagic ATEM switchers and a volunteer who has never run either.
What that person needs is one window: see every camera, pick the next shot,
let the camera follow the speaker, take it live. This plan adds that window to
GodwinMix and the four pieces behind it.

Three things set the bar. Tracking runs on Windows, Linux and macOS, on a
laptop with no GPU, in Rust, and nothing in it is tied to one camera or to the
PTZ use. GodwinMix keeps its own mixing and also drives one or more ATEM
switchers, either on its own or in step with the programme. Every action the
window takes is a protocol method or a plugin tool, so an agent can do all of
it too.

## The pieces

| Piece | Where | Branch | Needs GStreamer |
|---|---|---|---|
| Tracking engine | `crates/godwinmix-track/` | `feat/camera-track` | no |
| ATEM client | `crates/godwinmix-atem/` and `plugins/atem/` | `feat/camera-atem` | no |
| PTZ drivers and the follow loop | `crates/godwinmix-ptz/` and `plugins/ptz/` | `feat/camera-ptz` | only the plugin's frame input |
| Camera controls in Sources and Preview | `ui/panels/camera/` | `feat/camera-ui` | no |

Each branch starts from `feat/camera-control`, where this file lives, and goes
back into it. `main` gets the whole thing in one merge once the four work
together against a real core.

## 1. `godwinmix-track`: the tracking engine

A library crate with no media stack and no async runtime. It takes frames as
plain pixel slices and gives back what is in them and where a camera should
point. Everything that wants to know where people are uses it: the PTZ follow
loop, a digital crop that makes a virtual camera out of one wide 4K shot, the
director choosing who to cut to, the replay finding a moment, and agents asking
what a camera sees.

Layers, each usable alone:

1. **Detect.** A `Detector` trait: `fn detect(&mut self, frame: FrameRef) -> Vec<Detection>`.
   `FrameRef` is width, height, stride and a pixel format (RGB8, BGRA8, NV12,
   I420, GRAY8) over a borrowed slice, so a framebus slot or a decoded JPEG
   goes in without a copy. Backends:
   * `tract`: an ONNX person detector run by `tract-onnx`, which is pure Rust.
     The model must be Apache or MIT licensed: YOLOX nano or NanoDet Plus for
     boxes, MoveNet Lightning for keypoints. No Ultralytics YOLO, its AGPL
     licence does not fit an Apache project. The model file is downloaded on
     first use into the data directory and checked by its SHA256, never
     committed.
   * `motion`: frame difference and blobs, no model at all, for a Pi or as
     the fallback while the model loads.
   * Room for a platform backend later (Apple Vision, Windows ML) behind a
     feature, never the default.
2. **Track.** A multi object tracker in the ByteTrack style: a constant
   velocity Kalman filter per track, matching by IoU with a Hungarian
   assignment, high and low confidence passes, and a small colour histogram per
   track so a person who walks behind the pulpit gets the same id back. Stable
   ids are slugs, `person-3`, per the repository rules.
3. **Choose.** Who to follow: a locked id, the largest, the most central, the
   one nearest a point the operator tapped, or the one who is speaking when an
   audio direction hint is given. Losing the target holds the shot for a set
   time, then goes to a fallback (stay, go wide, go to a preset).
4. **Frame.** Turn the chosen person into a target rectangle for a framing
   style: `close` (head and shoulders), `waist`, `full` (whole body), `wide`
   (the person and the space around them), plus headroom and a lead room in
   the direction of movement. Keypoints are used when the detector gives them,
   box proportions when it does not.
5. **Steer.** Two outputs from the same target. For a PTZ camera: pan, tilt
   and zoom velocities from a PD controller with a dead zone, a maximum speed,
   an ease in and ease out, and a hold so the camera does not hunt. For a
   digital crop: a smoothed rectangle per frame (critically damped spring) for
   the compositor.

Performance budget, checked by a criterion bench and a test that fails when
the numbers slip: the full chain under 15 ms per frame at 320 pixels wide on
one core of a five year old laptop, and 10 frames a second on a Raspberry Pi
4. Detection can run every Nth frame with the tracker filling in between.
Nothing allocates per frame after warm up.

Agent surface, served by whoever hosts the engine (the `ptz` plugin first):
`track/observe` (what is in this source now: boxes, ids, confidence), `track/follow`
(follow an id or a point with a style), `track/stop`, `track/state`.

Tests: tracker and framing on synthetic scenes (boxes that move, cross and
hide), the detector on a handful of committed test images under 100 KB each
with known people in them, and a replay test that runs a recorded box sequence
and checks the camera command stream is smooth (no sign flips inside the dead
zone, no speed over the limit).

## 2. `godwinmix-atem` and `plugins/atem`: driving ATEM switchers

A pure Rust client for the ATEM network protocol: UDP on port 9910, the hello
and session handshake, acknowledged packets with resend, and the command
blocks the switcher sends as its state. The protocol is not published by
Blackmagic. The community has documented it well; work from those notes and
from packet captures, write the reference into `docs/reference/atem-protocol.md`,
and say there which models and firmware it was checked against.

State kept per switcher: product and firmware, inputs with long and short
names, each M/E's programme and preview, transition style, rate and position,
upstream and downstream keyers, fade to black, aux outputs, macros, tally per
input, and audio levels when asked for.

Commands: programme and preview per M/E, cut, auto, the transition bar,
transition style and rate, each DSK on, off and auto, USK on air, fade to
black, aux source, run, stop and record a macro, and media player stills.

More than one switcher at once, each with its own slug (`atem-main`,
`atem-overflow`), because a site with two is normal.

How it fits the mixer, set per switcher:

* `own`: GodwinMix mixes, the ATEM is driven from switcher properties on its own.
* `follow`: a take in GodwinMix also cuts the ATEM to the mapped input, so one
  button drives both. The map says which GodwinMix source is which ATEM input.
* `lead`: a cut on the ATEM panel moves GodwinMix's programme to match, so
  the hardware panel stays in charge and GodwinMix records and streams.

ATEM tally feeds GodwinMix's tally and the source tiles' red and green
borders, so the operator sees the same thing whichever box cut.

Tests: a fake switcher in the test that speaks the handshake, sends a real
initial state dump (captured, committed as bytes, no serial numbers in it) and
checks every command's bytes. A packet loss test drops one packet in five and
checks state still converges.

## 3. `godwinmix-ptz` and `plugins/ptz`: moving cameras

Drivers behind one trait (`move`, `stop`, `zoom`, `focus`, `goto_preset`,
`save_preset`, `position`, `exposure`):

* VISCA over IP: Sony's UDP framing on 52381 with sequence numbers, and raw
  VISCA over TCP for the cameras that use that instead. Most PTZ cameras sold
  for churches answer one of the two.
* ONVIF PTZ: `ContinuousMove`, `AbsoluteMove`, `Stop`, presets, reusing the
  SOAP and WS-Security code in `plugins/ipcam/src/onvif/`.
* Canon's IP protocol for the CR-N series, whose web API most Canon PTZ
  cameras share.
* NDI PTZ through the `ndi` plugin later, behind the same trait.

The plugin is a `service` that owns the follow loop: it reads a camera's
picture (from the frame bus where there is one, otherwise the core's preview
frames, decoded at low size), runs `godwinmix-track`, and sends the steer
output to the driver. It exposes the joystick as a velocity call that stops on
its own if no new call arrives within 300 ms, so a dropped browser tab never
leaves a camera spinning.

Presets are saved per camera with a thumbnail taken at save time, so the
window can show them as pictures, not numbers.

Tests: a fake VISCA camera and a fake ONVIF PTZ service in the test that keep
a position and answer queries, checked against the follow loop on a recorded
box sequence.

## 4. `ui/panels/cameras`: the window

One panel, built so a volunteer can run a service after five minutes. Dark and
light themes from the existing tokens, large touch targets, works on a tablet.

* **Camera strip.** One card per camera: live picture, tracking boxes drawn
  over it with the followed person highlighted, a red border when on air and
  green when next (from GodwinMix or the ATEM, whichever cuts), the tracking
  state in words ("Following: full length").
* **Step one, pick what's next.** Tap a card.
* **Step two, frame it.** A round joystick you drag (with a gamepad and a
  Stream Deck through Companion too), a zoom rocker, framing chips Close,
  Waist, Full, Wide, a Track switch, tap a person in the picture to follow
  them, and the preset grid as thumbnails with a plus to save the current
  shot. Exposure and focus sit in a drawer.
* **Step three, take it live.** Cut, Fade, Black and Lower third, sent to
  GodwinMix, to the ATEM, or both, per the switcher's mode.
* **ATEM bar,** when a switcher is connected: its programme and preview
  buses, DSK and FTB buttons, a picker when there are two switchers, and a
  dot that says whether the link is up.
* **Pro view,** one toggle away: every bus, transition rate, keyers, aux and
  macros, for the person who knows what an M/E is.

Every control calls a protocol method or plugin tool. The panel is tested in
`ui/test/` against the fake camera and fake ATEM, and the how to pages carry
real screenshots.

## Order of work

1. This plan on `feat/camera-control`.
2. In parallel: the tracking engine, the ATEM client, the PTZ drivers, and the
   panel against fakes.
3. The `ptz` plugin's follow loop on top of the tracking engine.
4. Wiring: tally from the ATEM, `follow` and `lead` modes, the panel against a
   real core with the plugins installed.
5. Docs: how to add a PTZ camera, how to connect an ATEM, how auto tracking
   works, the reference pages. Update `PARITY.md`.
6. A trial on real hardware: a VISCA camera, a Canon CR-N camera and an ATEM,
   with the results written into each README.
