# ipcam

IP cameras, the kind on a church wall, a stage truss, a car park pole or a
doorbell.

| Provide | What it does |
|---|---|
| `ipcam/source` | a camera's MJPEG stream, or its snapshot picture asked for a few times a second, over HTTP or HTTPS, with its login |
| `ipcam/discover` | finds the ONVIF cameras on the network and hands the core each profile's RTSP address, ready to add |

Most cameras made in the last ten years speak ONVIF and RTSP, and for those
discovery is the whole job: the camera appears in the picker under **Streams
and feeds**, and the core pulls its RTSP stream as it pulls any other. This
plugin moves no media for them. MJPEG and snapshots are for the cameras that
have nothing else, and for a picture a second from a camera whose RTSP stream
is already taken by a recorder.

How to use it from the page is
[docs/how-to/add-an-ip-camera.md](../../docs/how-to/add-an-ip-camera.md). Every
setting is in [docs/reference/plugins-network.md](../../docs/reference/plugins-network.md#ipcamsource-and-ipcamdiscover).

## Build and install

```sh
./build                           # stage bin/gmx-ipcam
gmx plugin add ./plugins/ipcam    # runs ./build itself when bin/ is empty
```

## Tested

`cargo test -p gmx-ipcam` runs a camera inside the test: an HTTP server that
streams real JPEGs as MJPEG and answers a snapshot only to the right basic
login, and an ONVIF device that answers the WS-Discovery probe and serves
GetCapabilities, GetProfiles and GetStreamUri only to a correct WS-Security
password digest. What the source writes is decoded by GStreamer and counted.
The digest itself is checked against the worked example in the ONVIF
Application Programmer's Guide.

Not tested here: a real ONVIF camera (none on this network), and cameras
that want HTTP digest rather than WS-Security on their ONVIF service.
