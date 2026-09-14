# The local test rig

Everything needed to run the mixer end to end on your own machine: an RTMP
server, a synthetic camera, a page to use as a `web+` source, and the mixer on
a test config.

```sh
cargo build --release
dev/harness/up.sh          # or: dev/harness/up.sh --debug
dev/harness/down.sh
```

| | |
|---|---|
| UI | <http://127.0.0.1:8080> |
| RTMP in | `rtmp://127.0.0.1:1935/live/cam1` |
| Programme out | `rtmp://127.0.0.1:1935/live/program` |
| Test pages | <http://127.0.0.1:8090> |
| Logs | `dev/harness/logs/` |

## What it starts

* **mediamtx**, as the RTMP server. `up.sh` downloads the release binary for
  your platform into `dev/harness/bin/` on first run, which is git ignored. One
  already on `PATH` is used instead. Pin a version with `MEDIAMTX_VERSION=v1.9.3`.
* **cam1**, SMPTE bars with a 440 Hz tone, published over RTMP by ffmpeg. The
  tone matters: the audio checks tell the camera from an ad clip by its
  spectral centroid, so a silent camera makes half the rig untestable. Without
  ffmpeg installed the rig still comes up and cam1 sits in `connecting`, which
  is a useful thing to test against too.
* **A page server** on 8090, serving `browser/test` when that is in the tree
  and `dev/harness/page` otherwise. The page has a second counter, a sweeping
  bar and a hue that changes once a second, three different rates, so whichever
  one you are watching tells you something about the frame in front of you.
* **The mixer** on `dev/harness/test.toml`: 720p30, `allow_exec_sources = true`,
  cam1 as a source and the local RTMP server as the output.

## Things worth knowing

* `GST_DEBUG=3 dev/harness/up.sh` puts GStreamer's own logging in
  `logs/gst.log` rather than on your terminal.
* Sources added while it runs are saved to `test.runtime.toml`, which is git
  ignored, and come back on the next `up.sh`. Delete it to start clean.
* `down.sh` also removes any `gmx-browser-*` containers. A sidecar container
  outlives a mixer that was killed rather than asked to stop, and each one
  costs a core.
* `dev/onair.sh <recording.flv>` measures what actually went on air (picture
  regions and audio level per five seconds) from an RTMP recording.

`gmx harness up` will do all of this without a shell script. This is the
version that exists.
