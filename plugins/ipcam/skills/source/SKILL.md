---
name: ipcam-source
description: Add an IP camera to GodwinMix. Use when the operator names a network camera, a doorbell, a PTZ camera, a security or surveillance camera, gives an http:// address ending in .mjpg, .cgi or .jpg, or asks to find the cameras on the network. Covers MJPEG streams, snapshot pictures and ONVIF discovery of RTSP cameras.
---

# ipcam/source and ipcam/discover

Three ways a network camera gets in, in the order to try them:

1. **Discovery.** `device.discover {}` lists every ONVIF camera on the LAN,
   one candidate per profile, as an `hls/source` with its `rtsp://` address.
   Add one with `source.add` and the candidate's params. A camera that wants
   a login is named in the `ipcam/discover` health; put the cameras' user and
   password in the plugin's settings (`plugin.configure`) and discover again.
2. **An RTSP address** from the camera's manual:
   `source.add {id: "door", uri: "rtsp://admin:pw@192.168.1.64:554/stream1"}`.
   The core opens it as a stream; this plugin is not involved.
3. **HTTP**, for cameras with no RTSP, or where a picture a second is enough:

```
source.add {id: "gate", type: "ipcam/source", uri: "http://192.168.1.64/video.mjpg"}
source.add {id: "yard", type: "ipcam/source", uri: "http://192.168.1.65/snapshot.jpg", fps: 2, user: "admin", password: "..."}
```

## Things to know

* MJPEG has no sound. Snapshots have no sound either.
* A snapshot that fails is skipped and the last picture stays; health says
  why the last one failed.
* `password` is a secret: it is stored encrypted and never returned.
