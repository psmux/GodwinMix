# ptz

PTZ cameras for GodwinMix, as a `service` plugin: VISCA over IP, raw VISCA
over TCP, ONVIF and Canon CR-N control, a joystick that stops by itself,
presets saved with a picture, and a follow loop that keeps a person in shot
with [`godwinmix-track`](../../crates/godwinmix-track).

* Tool calls are answered concurrently, and each camera has a motion lane of
  its own where the protocol allows a second connection, so `move`, `stop`
  and `zoom` never wait behind a slow `preset_save` or `position`.
* The follow loop finds people with YOLOX nano (the `detector` setting:
  `yolox`, `yolox+pose` or `motion`), with the motion detector standing in
  while the model downloads or loads and if it cannot be had.
* It tells the tracker how the camera itself is moving, from the speed the
  camera was told and the camera's `optics`, so a pan is not read as
  everyone walking.

* [Add a PTZ camera](../../docs/how-to/add-a-ptz-camera.md)
* [Let a camera follow the speaker](../../docs/how-to/auto-track-a-speaker.md)
* [The reference](../../docs/reference/ptz.md): every tool, the settings, the
  driver trait
* [The drivers](../../crates/godwinmix-ptz/README.md): which documents each
  protocol follows, and what is not yet checked on hardware

```sh
cargo test -p gmx-ptz           # the tools against a fake camera and a fake mixer
cargo bench -p gmx-ptz          # what a frame and a command cost
cargo build -p gmx-ptz --no-default-features   # the motion detector only, no model
./build                         # build and stage bin/gmx-ptz
gmx plugin add ./plugins/ptz
```

Or by hand, one tool call per line, with no mixer at all:

```sh
gmx-ptz --camera cam-demo:fake
camera_list {}
move {"pan": 0.5}
position {}
```

No GStreamer, no async runtime. On Linux and macOS the frame bus is read
without GStreamer too. The YOLOX model is fetched once into the models
directory (`~/.godwinmix/models` by default) the first time a follow runs.

## Hardware trial

Not yet run against a real camera. Every protocol is tested against a fake
camera that decodes what is sent with its own code; the README of
[`godwinmix-ptz`](../../crates/godwinmix-ptz/README.md) lists what that
cannot prove. Results from the trial land here.
