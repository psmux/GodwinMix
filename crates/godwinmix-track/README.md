# godwinmix-track

The tracking engine: frames in as borrowed pixel slices, people and camera
commands out. Five layers, each usable alone:

| Layer | Type | Does |
|---|---|---|
| Detect | `Detector`: `MotionDetector`, `TractDetector` | boxes in a `FrameRef` (RGB8, BGRA8, NV12, I420, GRAY8) |
| Track | `Tracker` | stable ids (`person-3`), Kalman filter, Hungarian matching, colour re-identification |
| Choose | `Chooser` | locked, largest, central, nearest a point, nearest the voice; hold then fall back |
| Frame | `frame_target` | close, waist, full or wide, with headroom and lead room |
| Steer | `PtzSteer`, `CropSteer` | pan, tilt and zoom speeds; a smoothed crop |

`Follower` runs all five per frame. Pure Rust, no media stack, no async
runtime; without features it depends on the standard library alone. The
`tract` feature adds the YOLOX nano person detector (Apache 2.0), fetched on
first use and checked by SHA256.

On a PTZ camera the picture moves because the camera does. Give
`FollowConfig::camera` the camera's `CameraOptics` and the follower turns
the speed it sends into an `EgoMotion`: the tracker carries every box with
the picture (`Tracker::update_with_camera`), so a pan is not read as
everyone walking, and the motion detector rests while the camera moves.
`tests/ego.rs` runs it in a closed loop with a simulated camera that pans
and zooms.

The example and the bench build for `aarch64-unknown-linux-musl` as they
are, for a Raspberry Pi; the `tract` feature then needs a C compiler for
the target. The commands are in the reference, under "For a Raspberry Pi".

* How it works: [`docs/explanation/auto-tracking.md`](../../docs/explanation/auto-tracking.md)
* The surface: [`docs/reference/track.md`](../../docs/reference/track.md)
* Using it: [`docs/how-to/track-people-in-a-picture.md`](../../docs/how-to/track-people-in-a-picture.md)

```sh
cargo test -p godwinmix-track                       # no model, no network
cargo test -p godwinmix-track --features tract      # fetches the model once
cargo bench -p godwinmix-track --features tract
cargo bench -p godwinmix-track -- motion            # only the names with "motion"
cargo run --release -p godwinmix-track --example follow
```

## Numbers

`cargo bench -p godwinmix-track --features tract` on 2026-11-11, one core of
an AMD Ryzen 9 9950X3D, Windows 11, Rust 1.99.1, with the workspace's bench
profile (opt-level `s`, fat LTO; opt-level 3 for every crate was measured too
and is about 4% faster, not worth a profile of its own). Each time is the median of 30 samples taken by the bench's own harness
(`benches/chain/harness.rs`), measured again after the camera motion
work; the earlier numbers were criterion's and agree within a few per cent. The motion rows use
a made up 320 by 180 RGB scene with five people walking; the model rows use
the committed photograph `tests/images/two-people.jpg` scaled to 320 by 240.

| Stage | Time a frame |
|---|---|
| detect: motion, 320x180 | 49.8 µs |
| detect: YOLOX nano at 320x256 | 7.69 ms |
| detect: YOLOX nano at 320x320 | 10.3 ms |
| detect: YOLOX nano at 416x416 | 18.4 ms |
| detect: YOLOX nano 320x256 and MoveNet Lightning on one person | 19.8 ms |
| track: 5 people | 0.59 µs |
| choose, frame and steer | 0.10 µs |
| whole chain, motion | 54.0 µs |
| whole chain, YOLOX nano 320x256, detect every frame | 6.88 ms |
| whole chain, YOLOX nano 320x256, detect every third frame | 2.31 ms |
| whole chain, YOLOX nano every frame, MoveNet every third | 10.6 ms |

The budget is 15 ms a frame at 320 wide, and every whole chain row is under
it. MoveNet costs about 12.5 ms a person, so keypoints on every frame are
not: run it on one detection in three (`PoseDetector::every`), and the
tracker carries each person's keypoints along with their box in between.
The quantised MoveNet build was tried and is slower in tract (120 ms) and
less accurate, so the fp32 one is used.

### On a Raspberry Pi 4

One core of a Raspberry Pi 4 Model B, a static `aarch64-unknown-linux-musl`
build made with `cargo zigbuild --release --features tract`, on 2026-11-11.
The bench was the criterion one of that day; the YOLOX row is the `follow`
example's own per frame time.

| Stage | Time a frame |
|---|---|
| detect: motion, 320x180 | 300 µs |
| track: 5 people | 4.6 µs |
| choose, frame and steer | 0.67 µs |
| whole chain, motion | 325 µs |
| whole chain, YOLOX nano at 320 wide, a frame it detects on | 107 ms |

So a Pi follows on motion with room to spare, and with YOLOX it detects
about nine times a second on one core. Following at 10 frames a second and
detecting on one in three costs about a third of a core.

`tests/budget.rs` fails when the motion chain allocates after warm up or, in
a release build, goes over the budget. The tract backends allocate inside
tract on each run; nothing else does.
