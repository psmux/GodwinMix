# GodwinMix in a container

`Dockerfile` builds the mixer from source in two stages and leaves a Debian
trixie runtime carrying the binary, the example config and the GStreamer plugin
sets the pipelines use. There is no artifact to fetch by hand first: a clone and
`docker build` is the whole recipe.

```sh
# From the root of the repository. The build context is the repository,
# because the builder stage compiles the source.
docker build -f deploy/docker/Dockerfile -t godwinmix .
```

Or take the published image, which CI builds on every tag:

```sh
docker pull ghcr.io/psmux/godwinmix:latest
```

## Trying it, with a local RTMP server

`docker-compose.yml` brings up mediamtx as the RTMP destination next to the
mixer, so nothing needs a stream key.

```sh
docker compose -f deploy/docker/docker-compose.yml up --build
```

The mixer is on http://localhost:8080 and the programme comes back out as HLS
at http://localhost:8888/live/program. The quickstart in the repository README
is the five command version of this.

## Running it for real

```sh
docker run -d --name godwinmix \
  --restart unless-stopped \
  --shm-size 1g \
  -p 127.0.0.1:8080:8080 \
  -e GODWINMIX_TOKEN="$(cat /etc/godwinmix/token)" \
  -v /srv/godwinmix/config:/etc/godwinmix \
  -v /srv/godwinmix/media:/var/lib/godwinmix/media \
  ghcr.io/psmux/godwinmix:latest
```

* `/etc/godwinmix` holds `godwinmix.toml` and the `godwinmix.runtime.toml` the
  mixer writes beside it when sources are added over the API. Mount nothing
  there and the entrypoint copies the shipped default in, which is enough to
  start and be driven over the API. A bind mounted directory has to be
  writable by the container's user first (`sudo chown -R 10001:10001
  /srv/godwinmix/config`), because the mixer does not run as root and a
  directory the daemon created for you is owned by root.
* `/var/lib/godwinmix/media` is the ad clip library and anything else played
  from a file.
* `GODWINMIX_TOKEN` overrides `[control] token` in the config. Set it. The
  entrypoint prints a warning when neither is present.
* `--shm-size 1g` is for WebKit. Docker's 64 MB default is not enough for a
  page with video in it.
* The port is published on loopback. Put a reverse proxy with TLS in front of
  it before it answers anything else. See [../README.md](../README.md).

The image is also the CLI, so no second install is needed to drive it:

```sh
docker exec godwinmix gmx ctl status
docker run --rm ghcr.io/psmux/godwinmix ctl --url https://mixer.example.com --token "$TOK" status
```

## Build arguments

| Argument | Default | What it does |
|---|---|---|
| `WITH_X264` | `1` | Installs `gstreamer1.0-plugins-ugly` and keeps only `libgstx264.so` out of it. `x264enc` is the only software H.264 encoder the mixer's probe knows, so an image built with `0` needs a hardware encoder (VA or NVENC) and refuses to start without one. Build with `0` where that package's licensing is a problem. |
| `WITH_CEF` | `1` | Builds and ships the Chromium browser sidecar and its runtime. Web pages run outside the mixer process. |
| `WITH_WPE` | `0` | Installs `gstreamer1.0-wpe` and the X and Mesa pieces it needs, so a `web+` source renders through WPE WebKit with no sidecar. Costs about 120 MB. Used only when no CEF sidecar is installed. |

```sh
docker build -f deploy/docker/Dockerfile --build-arg WITH_CEF=0 --build-arg WITH_WPE=0 -t godwinmix:slim .
```

What is deliberately not in the image: `gstreamer1.0-plugins-ugly` as a whole
(only the x264 element survives the install), and the CEF browser sidecar. The
sidecar carries a Chromium distribution of about a gigabyte and is moving to
its own repository as `gmx-browser`; installing it next to the binary and
naming it under `[browser] sidecar` is how it joins this image.

## GPU

With the NVIDIA driver and the NVIDIA container toolkit on the host, give the
container the GPU (`--gpus all`, or the compose
`deploy.resources.reservations.devices` block, with
`NVIDIA_DRIVER_CAPABILITIES=compute,video,graphics,utility`) and the probe
picks `nvh264dec` and `nvh264enc` on its own, because `hardware.decode` and
`hardware.encode` are `auto`. The same image on a host without the driver runs
on `avdec_h264` and `x264enc` and nothing else changes.

For VA (Intel or AMD), pass the render node: `--device /dev/dri:/dev/dri`, and
add the `gstreamer1.0-vaapi` or `va` plugin to the runtime stage. It is not in
the default image because it is dead weight on every host that has no Intel GPU.

`nvh264enc` accepts NV12 and RGB formats, not the canvas's I420, which is why
the mixer converts into the encoder's format before it.

## What it took to run a browser in a container

Kept from the first version of this image, because every one of these cost a
day and none of them are obvious.

* Xvfb needs `xfonts-base`. Without it the X server dies at once with no
  message and the browser reports "Missing X server or $DISPLAY".
* WPE's web process sandbox is bubblewrap, which cannot set up its namespaces
  in an unprivileged container, hence
  `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` in the entrypoint. The container
  is the sandbox.
* GL platform has to be pinned to EGL. Under GLX, which is the X11 default, the
  mixer's GL context is not one WPE can share and wpesrc fails with
  "EGL_KHR_image_base is not supported".
* On Debian trixie, pinned to EGL, wpesrc has still been seen to die inside
  WPEBackend-fdo's Wayland dispatch. That is a packaging problem between
  WPEBackend-fdo and wpewebkit rather than a configuration one. When a `web+`
  source fails in this image with the browser process gone, that is what
  happened. The default image now ships the CEF sidecar to avoid that path.
* `chrome-sandbox` from a CEF distribution must be root owned and setuid, which
  is why installing the sidecar is a step of its own and not a `COPY`.

## Updating

Pull or rebuild, then `docker compose up -d` or `docker restart godwinmix`. The
programme stops for the length of the restart. Sources and outputs come back
from `godwinmix.runtime.toml`.
