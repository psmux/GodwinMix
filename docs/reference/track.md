# Tracking engine

The crate `godwinmix-track`: detect, track, choose, frame and steer. Why it is
built this way is in [the explanation](../explanation/auto-tracking.md).

Coordinates are normalised everywhere: 0 is the left or top edge of the frame
and 1 the right or bottom, whatever its size in pixels. Times are
`std::time::Instant`, passed in by the caller, so a recorded sequence replays
exactly. Nothing in the crate starts a thread or needs an async runtime.

## Features

| Feature | Default | What it adds |
|---|---|---|
| `tract` | off | `detect::tract::TractDetector`, the ONNX person detector, and the model download |

Without features the crate depends on the standard library alone.

## Frames

```rust
pub enum PixelFormat { Rgb8, Bgra8, Nv12, I420, Gray8 }

pub struct FrameRef<'a> {
    pub width: u32,
    pub height: u32,
    pub stride: usize,          // bytes per luma or packed row
    pub format: PixelFormat,
    pub data: &'a [u8],         // every plane, one after another
}

impl<'a> FrameRef<'a> {
    pub fn new(width: u32, height: u32, stride: usize, format: PixelFormat, data: &'a [u8]) -> Result<Self, FrameError>;
    pub fn packed(width: u32, height: u32, format: PixelFormat, data: &'a [u8]) -> Result<Self, FrameError>;
    pub fn validate(&self) -> Result<(), FrameError>;
    pub fn aspect(&self) -> f32;
    pub fn luma_at(&self, x: u32, y: u32) -> u8;
    pub fn rgb_at(&self, x: u32, y: u32) -> [u8; 3];
    pub fn downscale_luma(&self, dst: &mut [u8], dw: usize, dh: usize);
    pub fn downscale_rgb(&self, dst: &mut [u8], dw: usize, dh: usize);
}
```

`Nv12` keeps its interleaved chroma plane at the luma stride; `I420` keeps
each chroma plane at half the luma stride. YUV is read as BT.601 limited
range. A frame built field by field is checked by every reader in the crate;
one that fails `validate` gives no detections and the tracker coasts.
`FrameError` says how many bytes the frame needs and how many it got.

## Geometry

```rust
pub struct Point { pub x: f32, pub y: f32 }
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }   // top left and size
```

`Rect` has `FULL`, `from_center`, `from_pixels`, `cx`, `cy`, `center`,
`right`, `bottom`, `area`, `intersection`, `iou`, `contains`, `clamp_inside`
and `is_finite`.

## Detect

```rust
pub trait Detector {
    fn detect(&mut self, frame: &FrameRef) -> Vec<Detection>;
    fn detect_into(&mut self, frame: &FrameRef, out: &mut Vec<Detection>);  // reuses `out`
    fn reset(&mut self) {}                                                   // after a cut or a move
    fn camera_motion(&mut self, ego: EgoMotion) {}                           // how the camera moves now
}

pub struct Detection { pub rect: Rect, pub score: f32, pub class: Class, pub keypoints: Option<Keypoints> }
pub enum Class { Person, Motion }
pub struct Keypoints(pub [Keypoint; 17]);   // COCO order, see `Joint`
pub fn non_max_suppression(dets: &mut Vec<Detection>, iou: f32);
```

`Box<dyn Detector + Send>` is a `Detector` too.

### `MotionDetector`

```rust
MotionDetector::new(MotionConfig) / MotionDetector::default()
```

| `MotionConfig` field | Default | Meaning |
|---|---|---|
| `width` | 160 | pixels the frame is scaled to first |
| `threshold` | 24 | brightness change, 0 to 255, that counts |
| `learn_shift` | 5 | the background learns 1/32 of each frame |
| `block` | 4 | changed pixels are counted in 4 by 4 blocks |
| `block_fill` | 0.25 | fraction of a block that must change |
| `min_area` | 0.003 | smaller boxes are dropped |
| `max_coverage` | 0.5 | above this the background starts again |
| `max_camera_speed` | 0.02 | frames a second (`EgoMotion::speed`); faster, it rests |

Told by `camera_motion` that the camera moves faster than
`max_camera_speed`, it returns no detections and leaves its background
alone; on the first frame after the camera is still it learns the new view,
and finds what moves again from the frame after. Without it, a pan reads as
one box the size of the picture, or, past `max_coverage`, as a fresh
background every frame.

Detections are `Class::Motion`, scored 0.55 to 0.95 by how full the box is.
The first frame, and the first after a reset, gives none.

### `TractDetector` (feature `tract`)

```rust
TractDetector::new(TractConfig) -> Result<TractDetector, ModelError>
```

| `TractConfig` field | Default | Meaning |
|---|---|---|
| `model` | `YOLOX_NANO` | the model and its SHA256 |
| `dir` | `None` | where the model is kept; `None` is `models_dir()` |
| `input` | `(416, 416)` | network size, multiples of 32; `(320, 192)` suits a 16:9 frame at 320 wide |
| `score_threshold` | 0.3 | least person score kept |
| `nms_iou` | 0.45 | overlap folded into the best box |

`new` blocks on the network the first time, so call it off any media thread.
The model is `yolox_nano.onnx` from the YOLOX 0.1.1rc0 release, Apache 2.0,
3.6 MB, SHA256 `c789161ed43c8269fcd4e67c67eeeb4e80c622da2eb296a20bc6007bd18a0b7d`.
It is checked on every load. `models_dir()` is `GODWINMIX_MODELS` when set,
else `models` under `GODWINMIX_HOME`, else `~/.godwinmix/models`. To run
offline, copy the file there by hand.

### `PoseDetector` (feature `tract`)

```rust
PoseDetector::new(inner: D, max_people: usize, dir: Option<PathBuf>) -> Result<PoseDetector<D>, ModelError>
pub every: u32    // run MoveNet on one call in this many, 1 by default
```

Wraps any box detector and adds `keypoints` to its best `max_people`
detections with MoveNet single pose Lightning (Apache 2.0, the Xenova ONNX
conversion pinned to revision `ed0f314b`, 9.4 MB, SHA256
`1ad4f8d6c2f776a9967db3993c9ca740bc350104f9d37c151dc183fc29a464ad`, file
`movenet_lightning.onnx`). It costs about 12.5 ms a person on one core, so
set `every` to 3 to stay inside 15 ms a frame. On the calls in between, and
for detections without keypoints, the tracker keeps each track's last
keypoints and moves them with its box.

`ModelError` is one of `Fetch` (offline or moved; copy the file by hand or use
the motion backend), `Hash` (the file is not the one expected; delete it),
`Io` and `Load`. tract allocates inside each run, so this backend is the one
part of the chain that allocates per frame.

## Track

```rust
pub struct Tracker;
impl Tracker {
    pub fn new(cfg: TrackerConfig) -> Self;
    pub fn update(&mut self, dets: &[Detection], t: Instant) -> &[Track];
    pub fn update_with_frame(&mut self, dets: &[Detection], frame: Option<&FrameRef>, t: Instant) -> &[Track];
    pub fn coast(&mut self, t: Instant) -> &[Track];   // a frame with no detector run
    pub fn update_with_camera(&mut self, dets: &[Detection], frame: Option<&FrameRef>, t: Instant, ego: EgoMotion) -> &[Track];
    pub fn coast_with_camera(&mut self, t: Instant, ego: EgoMotion) -> &[Track];
    pub fn tracks(&self) -> &[Track];
    pub fn get(&self, id: TrackId) -> Option<&Track>;
    pub fn reset(&mut self);
}

pub struct TrackId(pub u32);        // Display and FromStr as `person-3`
pub enum TrackState { Tentative, Confirmed, Lost }
pub struct Track {
    pub id: TrackId, pub rect: Rect, pub velocity: Point /* per second */,
    pub score: f32, pub class: Class, pub keypoints: Option<Keypoints>,
    pub state: TrackState, pub hits: u32, pub first_seen: Instant, pub last_seen: Instant,
}
```

The tracks returned are confirmed and lost ones, never tentative. A matched
detection without keypoints keeps the track's last keypoints, moved with the
box. A lost
track's box is where its filter thinks it went. Pass the frame to
`update_with_frame` for the colour histograms that bring a hidden person back;
without it they come back only when they reappear close by.

On a moving camera, `update_with_camera` and `coast_with_camera` carry every
box, lost ones too, with the picture by `ego` over the step before matching.
A pan then does not show as velocity on everyone, a track's `velocity` stays
the person's own, and a person the pan takes out of view is waited for where
they really are.

## Camera motion

```rust
pub struct EgoMotion { pub dx: f32, pub dy: f32, pub zoom: f32 }   // frames a second; log scale a second
impl EgoMotion {
    pub const STILL: EgoMotion;
    pub fn speed(&self) -> f32;                  // the larger shift, plus half the zoom
    pub fn over(&self, dt: f32) -> (f32, f32, f32);   // shift and scale over dt
}

pub struct CameraOptics { pub hfov_deg: f32, pub vfov_deg: f32, pub pan_deg_s: f32, pub tilt_deg_s: f32, pub zoom_rate: f32 }
impl CameraOptics {
    pub fn ego(&self, v: PtzVelocity) -> EgoMotion;   // pan right moves the picture left
}
```

`EgoMotion` is how the picture moves because the camera moves: everything in
it shifts by `dx`, `dy` a second and grows by `e^zoom` a second about the
centre. `CameraOptics` turns a PTZ speed into one: `dx = -pan * pan_deg_s /
hfov_deg`, `dy = tilt * tilt_deg_s / vfov_deg`, `zoom = zoom * zoom_rate`.
The default (30 by 17 degrees, 60 and 30 degrees a second, 0.7) suits a 20x
camera at a mid zoom.

| `TrackerConfig` field | Default | Meaning |
|---|---|---|
| `high_score` | 0.5 | confident detections |
| `low_score` | 0.1 | below this a detection is ignored |
| `new_track_score` | 0.6 | needed to start a track |
| `match_iou` | 0.2 | least overlap for a confident match |
| `low_match_iou` | 0.4 | least overlap for a weak detection to keep a track |
| `confirm_hits` | 2 | matches before a track is trusted; the first frame's are trusted at once |
| `max_lost` | 5 s | how long a lost track can come back |
| `reid_similarity` | 0.75 | how alike the colours must be to come back |
| `reid_distance` | 0.6 | how far from where it was lost |
| `appearance_weight` | 0.3 | how much colour counts beside overlap |

## Choose

```rust
pub enum Choose { Locked(TrackId), Largest, Central, Nearest(Point), Speaker { x: f32 } }
pub enum Fallback { Stay, Wide, Preset(Arc<str>) }
pub enum FollowState { Idle, Following, Holding, Fallback(Fallback) }
pub struct ChooseConfig { pub lost_hold: Duration /* 2 s */, pub fallback: Fallback /* Stay */, pub switch_margin: f32 /* 1.5 */ }

impl Chooser {
    pub fn new(policy: Choose, config: ChooseConfig) -> Self;
    pub fn set_policy(&mut self, policy: Choose);
    pub fn choose(&mut self, tracks: &[Track], t: Instant) -> Choice;   // target, rect, state
}
```

`Locked` never picks anybody else; when its id comes back it follows again.
`Fallback(Preset(slug))` is for the PTZ host to act on.

## Frame

```rust
pub enum Framing { Close, Waist, Full, Wide }          // slugs: close, waist, full, wide
pub struct FrameStyle { pub framing: Framing, pub headroom: f32 /* 0.10 */, pub lead_room: f32 /* 0.5 s */,
                        pub aspect: Option<f32> /* the frame's */, pub min_height: f32 /* 0.15 */ }
pub fn frame_target(track: &Track, style: &FrameStyle, frame_aspect: f32) -> Rect;
```

The result may reach outside the frame; a PTZ camera can pan there, and the
crop clamps it.

## Steer

```rust
pub struct PtzVelocity { pub pan: f32, pub tilt: f32, pub zoom: f32 }   // each -1..=1
impl PtzSteer { pub fn new(PtzConfig) -> Self; pub fn step(&mut self, target: Option<Rect>, t: Instant) -> PtzVelocity; pub fn reset(&mut self); }
impl CropSteer { pub fn new(CropConfig) -> Self; pub fn step(&mut self, target: Option<Rect>, t: Instant) -> Rect; pub fn jump(&mut self, r: Rect); pub fn current(&self) -> Rect; }
```

Positive pan turns right, positive tilt looks up, positive zoom goes in.
`None` eases every axis to a stop. The speed never changes sign without
passing through zero.

| `PtzConfig` field | Default | Meaning |
|---|---|---|
| `gain`, `zoom_gain` | 2.0, 0.8 | speed per unit of error |
| `damping` | 0.15 | speed per unit of error change per second |
| `dead_zone`, `zoom_dead_zone` | 0.06, 0.15 | a still axis stays still below these |
| `settle` | 0.4 | a moving axis stops under this fraction of the dead zone |
| `max_speed` | 0.8 | |
| `max_accel` | 2.5 | speed change per second |
| `hold` | 600 ms | wait after settling before moving again |

`CropConfig` is `smooth_time` (0.6 s), `max_speed` (1.0 a second) and
`dead_zone` (0.03).

## Follow

```rust
impl<D: Detector> Follower<D> {        // D defaults to Box<dyn Detector + Send>
    pub fn new(detector: D, config: FollowConfig) -> Self;
    pub fn step(&mut self, frame: &FrameRef, t: Instant) -> FollowOutput;
    pub fn step_into(&mut self, frame: &FrameRef, t: Instant, out: &mut FollowOutput);   // no allocation
    pub fn follow(&mut self, id: TrackId);
    pub fn follow_point(&mut self, p: Point);    // locks whoever is nearest now
    pub fn set_policy(&mut self, policy: Choose);
    pub fn set_framing(&mut self, framing: Framing);
    pub fn stop(&mut self);                      // keeps tracking, stops steering
    pub fn reset(&mut self);
    pub fn set_camera_velocity(&mut self, v: PtzVelocity);   // what the camera really does, for the next step
    pub fn tracker(&self) -> &Tracker;
    pub fn detector_mut(&mut self) -> &mut D;
    pub fn config(&self) -> &FollowConfig;
}

pub struct FollowConfig { pub detect_every: u32 /* 1 */, pub policy: Choose /* Largest */, pub tracker: TrackerConfig,
                          pub choose: ChooseConfig, pub style: FrameStyle, pub ptz: PtzConfig, pub crop: CropConfig,
                          pub camera: Option<CameraOptics> /* None */ }

pub struct FollowOutput {
    pub target: Option<TrackId>,
    pub tracks: Vec<Track>,
    pub ptz: PtzVelocity,
    pub crop: Rect,             // smoothed, inside the frame
    pub shot: Option<Rect>,     // the aim before smoothing
    pub state: FollowState,
    pub detected: bool,         // the detector ran on this frame
    pub ego: EgoMotion,         // how the camera moved the picture over this step
}
```

With `camera: Some(optics)` the follower knows its camera moves. Each step
it takes the speed the camera was told last (its own last `ptz`, or what
`set_camera_velocity` gave when something else, an operator's joystick,
moves the camera), turns it into an `EgoMotion` with the optics, hands that
to the detector's `camera_motion` and the tracker's `update_with_camera`,
and reports it as `ego`. The optics' field of view is the one when the
follow starts; the follower integrates its own zoom from there (`reset`
starts again at 1), so a pan zoomed in twice as far moves the picture twice
as fast. `None`, the default, is for a fixed camera or a digital crop, where
the picture never moves.

While `Holding` the PTZ speeds ease to a stop and the crop holds. On
`Fallback(Wide)` and `Fallback(Preset)` the crop eases to the whole frame.

## The example

```sh
cargo run --release -p godwinmix-track --example follow                       # a made up scene
cargo run --release -p godwinmix-track --example follow -- clip.y4m --framing full
cargo run --release -p godwinmix-track --features tract --example follow -- frames/ --tract --every 3
```

### For a Raspberry Pi

The example and the bench build for `aarch64-unknown-linux-musl` as they
are. Without `tract` nothing in them is C, so Rust's own linker does it,
from any machine:

```sh
rustup target add aarch64-unknown-linux-musl
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld cargo build --release -p godwinmix-track --target aarch64-unknown-linux-musl --example follow --bench chain
```

This was checked from Windows 11. On the Pi itself the system `cc` usually
links it; where it does not, set the same variable. The bench is timed by a
small harness of its own rather than criterion, whose `alloca` dependency
compiles C.

With `--features tract`, `tract-linalg` assembles its aarch64 kernels with
the `cc` crate, so a C compiler and assembler for the target are needed. It
reads `CC_aarch64_unknown_linux_musl`, then `TARGET_CC`, then `CC`:

* On the Pi (64 bit Raspberry Pi OS): `sudo apt install musl-tools`, then
  `CC_aarch64_unknown_linux_musl=musl-gcc cargo build --release -p godwinmix-track --features tract --target aarch64-unknown-linux-musl --example follow --bench chain`.
  Without the variable, `cc` looks for `aarch64-linux-musl-gcc`, which
  Debian does not ship. If linking fails, add
  `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc`.
* From Windows, macOS or another Linux: `cargo zigbuild` with zig installed
  (`cargo install cargo-zigbuild`; zig from ziglang.org or `pip install
  ziglang`, in which case point `CARGO_ZIGBUILD_ZIG_PATH` at its `zig`), then
  `cargo zigbuild --release -p godwinmix-track --features tract --target aarch64-unknown-linux-musl --example follow --bench chain`.
  This was checked from Windows 11 and gives static aarch64 binaries; it was
  not run on a Pi from here.

It prints one line a frame: whether the detector ran, the track count, who is
followed and in what state, the PTZ speeds, the crop and the time taken. A
y4m file must be 4:2:0 or mono; `ffmpeg -i in.mp4 -vf scale=320:-2 -pix_fmt
yuv420p clip.y4m` makes one. A folder is read as `.jpg` and `.png` files in
name order at 10 frames a second.
