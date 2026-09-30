# Add an IP camera

A network camera, the kind fixed to a wall or a truss, reaches the mixer one of
three ways. Try them in this order.

## Let the mixer find it

Most cameras speak ONVIF, and the mixer can find those by itself.

1. Install the IP camera plugin once: in the page, open **Plugins** and add
   `ipcam`. It installs while the mixer runs.
2. Press **Add sources** and choose **Streams and feeds**. The cameras on this
   network are listed, one row per profile (a main stream and a smaller
   second stream, usually). Pick one.

Most cameras only say their stream addresses to a login. When that is so, the
list says which cameras need one. Open **Plugins**, the IP camera plugin's
settings, and put in the cameras' user name and password; then press
**Rescan**. The same login is tried on every camera.

The picture comes over RTSP and is decoded by the mixer on its own hardware
path; the plugin only found the address.

## Type the RTSP address

If the camera is on another network, or discovery does not list it, its manual
gives an RTSP address. Add it as an incoming stream:
`rtsp://admin:password@192.168.1.64:554/stream1`.

## MJPEG or a snapshot

Older and cheaper cameras, and some doorbells, only serve JPEG pictures over
HTTP.

1. In **Streams and feeds**, choose **Ipcam**.
2. Put in the camera's address, from its manual or its web page: an MJPEG
   address such as `http://192.168.1.64/video.mjpg`, or a snapshot address such
   as `http://192.168.1.64/snapshot.jpg`.
3. Put in its user name and password if it has a login.

A snapshot address is asked for 5 pictures a second unless you change it under
**Advanced**. If one request fails, the last picture stays on screen and the
source says why the last one failed. Neither kind carries sound.

## When it does not show

* **Nothing is found.** The camera may have ONVIF switched off (it is often a
  setting in the camera's own web page), or it is on another network segment:
  discovery does not cross routers. Type its RTSP address instead.
* **Found, but it says it needs a login even after you set one.** Some cameras
  keep a separate ONVIF user from their web login; add one in the camera's web
  page under ONVIF or Network.

Every setting is in the
[network plugins reference](../reference/plugins-network.md#ipcamsource-and-ipcamdiscover).
