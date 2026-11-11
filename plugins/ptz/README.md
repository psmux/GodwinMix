# ptz

PTZ cameras for GodwinMix, as a `service` plugin: VISCA over IP, raw VISCA
over TCP, ONVIF and Canon CR-N control, a joystick that stops by itself,
presets saved with a picture, and a follow loop that keeps a person in shot
with [`godwinmix-track`](../../crates/godwinmix-track).

* [Add a PTZ camera](../../docs/how-to/add-a-ptz-camera.md)
* [Let a camera follow the speaker](../../docs/how-to/auto-track-a-speaker.md)
* [The reference](../../docs/reference/ptz.md): every tool, the settings, the
  driver trait
* [The drivers](../../crates/godwinmix-ptz/README.md): which documents each
  protocol follows, and what is not yet checked on hardware

```sh
cargo test -p gmx-ptz           # the tools against a fake camera and a fake mixer
cargo bench -p gmx-ptz          # what a frame and a command cost
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
without GStreamer too.

## Hardware trial

Not yet run against a real camera. Every protocol is tested against a fake
camera that decodes what is sent with its own code; the README of
[`godwinmix-ptz`](../../crates/godwinmix-ptz/README.md) lists what that
cannot prove. Results from the trial land here.
