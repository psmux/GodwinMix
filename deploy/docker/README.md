# LiveboxMix in a container

The mixer as shipped: binaries only, no source. `Dockerfile` builds a Debian
trixie image with GStreamer (including `gstreamer1.0-wpe`), Xvfb for Chromium
to start against, Mesa for a GL context on a box without a GPU, and the
shared libraries the CEF sidecar needs. Put the two binaries and the CEF
distribution in `bin/` beside the Dockerfile before building:

```
deploy/docker/            (or wherever the recipe is copied)
  Dockerfile
  entrypoint.sh           Xvfb, then the wpesrc mixer on :8081 if wpe.toml exists, then the mixer on :8080
  bin/
    liveboxmix            the CI artifact for Linux (built on Ubuntu 24.04, glibc 2.39)
    liveboxmix-browser    the CEF sidecar, same artifact
    libcef.so, chrome-sandbox, *.pak, locales/, ...
                          a CEF 150.0.10 distribution with H.264 and AAC, flattened here
                          (Karere's cef-150.0.10-proprietary-codecs release, linux64 minimal)
```

Mount a config directory at `/etc/liveboxmix` holding `liveboxmix.toml` (start
from `liveboxmix.example.toml`; set `[control] token`) and, optionally,
`wpe.toml` for the wpesrc mixer, and a media directory at
`/var/lib/liveboxmix/media`. The mixer writes the sources it is given to
`liveboxmix.runtime.toml` next to its config and reloads them on restart.

```bash
docker build -t liveboxmix deploy/docker
docker run -d --name liveboxmix --shm-size 1g -p 127.0.0.1:8080:8080 \
  -v $PWD/config:/etc/liveboxmix -v $PWD/media:/var/lib/liveboxmix/media liveboxmix
```

## GPU

With the NVIDIA driver and the NVIDIA container toolkit on the host, give the
container the GPU (`--gpus all`, or the compose `deploy.resources.reservations.devices`
block, with `NVIDIA_DRIVER_CAPABILITIES=compute,video,graphics,utility`) and
the mixer's probe picks `nvh264dec` and `nvh264enc` on its own
(`hardware.decode/encode = "auto"`). The same image on a host without the
driver runs on `avdec_h264` and `x264enc`; nothing else changes. `nvh264enc`
accepts NV12 and RGB formats, not the canvas's I420, which is why the mixer
converts into the encoder's format before it.

## What it took to run a browser in a container

* Xvfb needs `xfonts-base`; without it the X server dies at once with no
  message and Chromium reports "Missing X server or $DISPLAY".
* `chrome-sandbox` must be root owned and setuid (the Dockerfile does it).
* WPE's web process sandbox is bubblewrap, which cannot set up its namespaces
  in an unprivileged container: `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1`.
* wpesrc does not work in this GPU-less container, and it is not a
  configuration matter. Under GLX (the default on X11) the mixer's GL context
  is not EGL and wpesrc fails "EGL_KHR_image_base is not supported"; pinned to
  EGL, in every window, API and Mesa driver variant tried, the process dies in
  WPEBackend-fdo's Wayland dispatch calling a null handler. That points at the
  trixie packaging of WPEBackend-fdo against wpewebkit. The wpesrc mixer still
  starts on :8081 so the arrangement is in place; a web source added to it
  fails. The two CEF methods, whole page and superimpose, both work in the
  container, on air with sound.

## Behind a proxy or a CDN

The UI is one HTML page with inline scripts and a WebSocket. A CDN that
rewrites scripts (Cloudflare's Rocket Loader) or caches HTML breaks it: put
`Cache-Control: no-store, no-transform` on every response and proxy the
WebSocket with its upgrade headers. `GET /login?token=<token>` is a good thing
for such a proxy to serve: a page that stores the token where the UI looks and
opens the UI, so a wrong token typed into the UI's prompt (which the page
discards, then retries its socket without one: "Disconnected from the mixer.
Retrying…") is not the only way in.

## Sound from the page

A page that plays video usually starts it muted and waits for a click. Nobody
is at the mixer's browser to click, so the browser announces itself as
`LiveboxMix/<version>` in its user agent (a page can start unmuted for it) and
presses the page's own unmute for it: every whole page gets a small script on
load (`browser/src/unmute.js`) that unmutes media elements and clicks a
control whose label reads "Enable Sound", "Unmute", "Tap for sound" or the
like, once, retrying for a minute while players appear. Superimposed pages
are left alone: there the mixer plays the media itself.

## Updating

Copy new `liveboxmix` and `liveboxmix-browser` into `bin/`, rebuild the image,
restart the container. Sources come back from `liveboxmix.runtime.toml`.
`dev/onair.sh <recording.flv>` measures what went on air (picture regions and
audio level per 5 s) from an RTMP recording.
