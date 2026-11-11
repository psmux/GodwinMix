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

* How it works: [`docs/explanation/auto-tracking.md`](../../docs/explanation/auto-tracking.md)
* The surface: [`docs/reference/track.md`](../../docs/reference/track.md)
* Using it: [`docs/how-to/track-people-in-a-picture.md`](../../docs/how-to/track-people-in-a-picture.md)

```sh
cargo test -p godwinmix-track                       # no model, no network
cargo test -p godwinmix-track --features tract      # fetches the model once
cargo bench -p godwinmix-track --features tract
cargo run --release -p godwinmix-track --example follow
```

## Numbers

`cargo bench -p godwinmix-track --features tract` on 2026-11-11, one core of
an AMD Ryzen 9 9950X3D, Windows 11, Rust 1.99.1, with the workspace's bench
profile (opt-level `s`, fat LTO). Frames are 320 by 180 RGB with five people.

| Stage | Time a frame |
|---|---|
| detect: motion | 48.7 µs |
| detect: YOLOX nano at 320x192 | 5.50 ms |
| detect: YOLOX nano at 320x320 | 9.85 ms |
| detect: YOLOX nano at 416x416 | 18.1 ms |
| track: 5 people | 0.55 µs |
| choose, frame and steer | 0.09 µs |
| whole chain, motion | 52.7 µs |
| whole chain, YOLOX nano 320x192, detect every frame | 5.30 ms |
| whole chain, YOLOX nano 320x192, detect every third frame | 1.92 ms |

The budget is 15 ms a frame at 320 wide. `tests/budget.rs` fails when the
motion chain allocates after warm up or, in a release build, goes over it.
The tract backend allocates inside tract on each run; nothing else does.
