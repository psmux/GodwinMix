# The isp server: quiz, simulator and mixer in one compose stack

What runs on the customer's Ubuntu 24.04 box (`ssh isp`, through its
Cloudflare tunnel), under `/opt/btq`. LiveboxMix is present there as binaries
only; its source stays in this repository.

```
/opt/btq
  docker-compose.yml   the stack below
  .env                 SESSION_SECRET for the quiz; not in any repository
  app/                 the quiz (BTQ-Project) source, built into an image
  simulator/           BTQ-Simulator source, built into an image
  www/                 test pages for the mixer (demo.html and its clips)
  lbx/
    Dockerfile         Debian trixie + GStreamer (with wpesrc) + Xvfb + CEF's needs
    entrypoint.sh      Xvfb, the wpesrc mixer on :8081, then the mixer on :8080
    bin/               liveboxmix, liveboxmix-browser (from the CI artifact
                       liveboxmix-Linux) and the CEF distribution with codecs
                       (Karere's cef_binary_150.0.10 linux64) flattened beside them
    config/            liveboxmix.toml (CEF and superimpose), wpe.toml (wpesrc),
                       and the runtime files the mixer writes
    media/             the mixer's media library
```

| service | what | reached as |
|---|---|---|
| db | postgres 16 | inside the stack |
| btq | the quiz, port 5001 | https://quiz.spinber.com |
| simulator | plays a championship forever (`--forever --end-live-matches`) | logs: `docker compose logs -f simulator` |
| mediamtx | RTMP in | inside the stack |
| hls | ffmpeg writing plain HLS of the programme | inside the stack |
| player | nginx: the player page and the HLS files | **https://stream.spinber.com/index.html** (HLS at `/hls/program.m3u8`, for VLC or OBS) |
| lbx | LiveboxMix, `:8080` mixer, `:8081` wpesrc mixer, loopback only | `ssh -L 8080:localhost:8080 isp`, then http://localhost:8080 |
| www | nginx with the demo pages | http://www/demo.html from inside the stack |

The mixer's UI has no login, so it is deliberately not on a public hostname.
Reach it through the SSH port forward above, or put it behind Cloudflare Access
before giving it a hostname.

## Updating the mixer

Download the `liveboxmix-Linux` artifact from the latest green run of
`.github/workflows/build.yml`, copy `liveboxmix` and `liveboxmix-browser` into
`/opt/btq/lbx/bin/` on the box, then `docker compose build lbx && docker compose
up -d lbx`. Sources come back from `lbx/config/liveboxmix.runtime.toml`.

## What was needed to make the browser run in a container

* Xvfb needs `xfonts-base`; without it the X server dies at once with no
  message and Chromium reports "Missing X server or $DISPLAY".
* WPE's web process sandbox is bubblewrap, which cannot set up its namespaces
  in an unprivileged container: `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1`.
* wpesrc does not work in this container, and it is not a configuration
  matter. Under GLX (the default on X11) the mixer's GL context is not EGL and
  wpesrc fails "EGL_KHR_image_base is not supported"; pinned to EGL, in every
  window, API and Mesa driver variant tried, the process dies in
  WPEBackend-fdo's Wayland dispatch calling a null handler (backtrace in the
  commit that added this line). That points at the trixie packaging of
  WPEBackend-fdo against wpewebkit rather than at anything this stack does.
  The wpesrc mixer still starts on :8081 so the arrangement is in place; a
  web source added to it fails. The two CEF methods, whole page and
  superimpose, are what the box runs, and both are verified on air there.
* The quiz server bound to `"localhost"`, which Node 20 resolves to `::1`
  only; it binds `0.0.0.0` now (or `HOST`).

## The GPU

The box has an RTX 3060. `gpu-install.sh` is what put the driver on it:
`nvidia-driver-580-server` (the production branch; the "recommended" 595-open
is the newest desktop branch) plus the NVIDIA container toolkit, registered
as Docker's `nvidia` runtime. Secure Boot is off on this machine, so Ubuntu's
DKMS module loads as built; a reboot replaced nouveau. The compose file gives
the `lbx` service every GPU with `compute,video,graphics,utility`, and the
mixer's probe then picks `nvh264dec` and `nvh264enc` on its own
(`hardware.decode/encode = "auto"`). The same image on a host without the
driver runs on `avdec_h264` and `x264enc`; nothing else changes.

`nvh264enc` accepts NV12 and RGB formats, not the canvas's I420, which is why
the mixer converts into the encoder's format before it (commit 99ebabd); the
first start on this machine failed to link the encoder without that.

## Watching the programme through Cloudflare

Three things stood between mediamtx's built-in HLS page and a viewer behind
Cloudflare, each found by loading the page in a browser rather than with curl:

* Cloudflare's Rocket Loader rewrote the page's inline script and the player
  never requested a playlist. Our page marks its scripts `data-cfasync="false"`.
* The zone caches generously and mediamtx sent `max-age=1800` on a live
  playlist. Playlists are served `no-store` and the player adds a unique query
  to every playlist request; segments are named uniquely and may be cached.
* mediamtx gates every media playlist behind a per-viewer session cookie set on
  a redirect, and through Cloudflare that cookie did not come back: 401 on
  every media playlist. The `hls` service writes plain HLS files with ffmpeg
  instead; nothing per viewer, nothing to negotiate.

hls.js 1.7 (as bundled by mediamtx) sat idle on this stream; the page ships
hls.js 1.5.20. Verified by rendering the public page in the server's own
Chromium (the CEF sidecar) and measuring the frames. Chrome does not start media
in a hidden tab, so a background tab shows a spinner until it is brought to the
front.
