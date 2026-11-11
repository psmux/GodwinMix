# Track people in a picture

For Rust code that has decoded frames and wants to know where the people are,
or where a camera should point. The crate is `godwinmix-track`; the surface
is in [the reference](../reference/track.md) and how it works in
[the explanation](../explanation/auto-tracking.md).

## Add the crate

Inside this workspace:

```toml
[dependencies]
godwinmix-track = { path = "../godwinmix-track" }
# or, with the ONNX person detector:
godwinmix-track = { path = "../godwinmix-track", features = ["tract"] }
```

## See it work first

```sh
cargo run --release -p godwinmix-track --example follow
```

runs a made up scene of somebody walking and prints a line a frame. Give it a
y4m file or a folder of images to try your own pictures, and add
`--features tract` before `--example` and `--tract` after the path to use the
person detector.

## Follow somebody

```rust
use std::time::Instant;
use godwinmix_track::{FollowConfig, FollowOutput, Follower, FrameRef, Framing, MotionDetector, PixelFormat};

let mut follower = Follower::new(MotionDetector::default(), FollowConfig::default());
follower.set_framing(Framing::Full);
let mut out = FollowOutput::default();

// For every decoded frame:
let frame = FrameRef::new(width, height, stride, PixelFormat::Nv12, &bytes)?;
follower.step_into(&frame, Instant::now(), &mut out);
camera.set_speeds(out.ptz.pan, out.ptz.tilt, out.ptz.zoom);   // a PTZ camera
compositor.set_crop(out.crop);                                  // or a digital crop
```

Keep one `FollowOutput` and call `step_into`, and nothing allocates once the
first few frames have gone by. Pass the time the frame was captured rather
than the time it arrived if you have it: the filters use the gaps between
frames.

## On a camera that moves

A PTZ camera the follower turns moves its own picture. Give the follower the
camera's optics, and it takes that motion out before tracking:

```rust
use godwinmix_track::{CameraOptics, PtzVelocity};

let optics = CameraOptics { hfov_deg: 30.0, vfov_deg: 17.0, pan_deg_s: 60.0, tilt_deg_s: 30.0, zoom_rate: 0.7 };
let mut follower = Follower::new(MotionDetector::default(), FollowConfig { camera: Some(optics), ..FollowConfig::default() });

// When something else moves the camera too, say what it really does:
follower.set_camera_velocity(PtzVelocity { pan: joystick.pan, tilt: joystick.tilt, zoom: 0.0 });
```

Without `set_camera_velocity` it takes the camera to do what the follower
last told it. Leave `camera` at `None` for a fixed camera or a digital crop.

## Use the person detector

```rust
use godwinmix_track::detect::tract::{TractConfig, TractDetector};

let detector = TractDetector::new(TractConfig { input: (320, 192), ..TractConfig::default() })?;
let mut follower = Follower::new(detector, FollowConfig { detect_every: 3, ..FollowConfig::default() });
```

The first `TractDetector::new` downloads the model, 3.6 MB, into
`~/.godwinmix/models` and blocks while it does, so make it on a thread of its
own and use the motion backend meanwhile. On a machine with no internet,
copy `yolox_nano.onnx` into that directory (or the one `GODWINMIX_MODELS`
names) by hand; the reference page has its address and SHA256.

`input: (320, 192)` suits a 16:9 frame: it is the frame at 320 wide with the
least padding. `detect_every: 3` runs the model on one frame in three and
lets the tracker fill in, which is what a Raspberry Pi needs.

## Follow one person

```rust
follower.follow_point(Point::new(0.7, 0.5));   // whoever is nearest where the operator tapped
follower.follow("person-3".parse()?);          // an id from out.tracks
follower.set_policy(Choose::Speaker { x: 0.2 });  // nearest where the voice comes from
follower.stop();                               // keep tracking, stop steering
```

## Only detect, or only track

Each layer works alone. To list who is in a frame:

```rust
let mut detector = MotionDetector::default();
let mut tracker = Tracker::default();
let dets = detector.detect(&frame);
for t in tracker.update_with_frame(&dets, Some(&frame), Instant::now()) {
    println!("{} at {:?}", t.id, t.rect);
}
```

`frame_target` gives the shot for one track and a framing without any
steering, and `PtzSteer` and `CropSteer` take any rectangle, so a host with
its own detector can use just the bottom of the chain.
