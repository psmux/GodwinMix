# The isp server: quiz, simulator and mixer in one compose stack

The step by step installation checklist for the whole thing (all three
projects, the tunnel, the binaries, the first start) is `docs/server-setup.md`
in the BTQ-Simulator repository. This file is the reasoning behind the pieces.

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
| director | follows the quiz: the live match's watch page on programme, the championship page between matches (`director/director.py`) | logs: `docker compose logs -f director` |
| mediamtx | RTMP in | inside the stack |
| hls | ffmpeg writing plain HLS of the programme | inside the stack |
| player | nginx: the player page and the HLS files | **https://stream.spinber.com/watch** (HLS at `/hls/program.m3u8`, for VLC or OBS) |
| lbx | LiveboxMix, `:8080` mixer (bearer token: `[control] token` in `lbx/config/liveboxmix.toml`, also `MIXER_TOKEN` in `.env` for the director), `:8081` wpesrc mixer | https://mixer.spinber.com with the token, or `ssh -L 8080:localhost:8080 isp` |
| www | nginx with the demo pages | http://www/demo.html from inside the stack |

The mixer's API and UI are behind a bearer token (see docs/agents.md); the UI
asks for it once. An agent anywhere drives it with `liveboxmix mcp --url
https://mixer.spinber.com --token ...` or plain HTTP with the Authorization
header.

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

The mixer's hostname goes through the same nginx (port 81 in the `player`
container, 8083 on the host) rather than straight to the mixer, so that every
response carries `Cache-Control: no-transform`: without it Cloudflare's Rocket
Loader rewrote the UI's inline script and the page painted once and went blank,
and the edge cached the page for half an hour. The WebSocket that carries the
state and the mosaic is proxied with its upgrade headers.

Signing a browser into the mixer: open `https://mixer.spinber.com/login?token=<token>`
once. It stores the token where the UI looks and opens the UI. Typing into the
UI's own prompt also works, but a wrong token typed there is discarded and the
page then retries its socket without one until it is reloaded, which reads as
"Disconnected from the mixer. Retrying…".

## The Go Live button

The quiz's admin panel (Championship Management) has a Live stream card with
Go Live and Off Air. Go Live turns the director on: the live match goes on
the stream and every following match follows by itself. Off Air cuts the
stream to black and stops following. The quiz server proxies these to the
director's control endpoint (`GET /state`, `POST /on`, `POST /off` on
`director:8090`, set as `DIRECTOR_URL` in the compose file); the browser never
touches the mixer. The card links to https://stream.spinber.com/watch.

## Sound from the page

A page that plays video usually starts it muted and waits for a click (the
quiz's watch page shows "Enable Sound"). Nobody is at the mixer's browser to
click, so two things take care of it. The director opens watch pages with
`?broadcast=1`, and the mixer's browser announces itself as `LiveboxMix/<version>`
in its user agent; the watch page treats either as a broadcast capture,
starts its video with sound, and does not show the control. For any other
site, the browser presses the page's own unmute for it: every whole page gets
a small script on load that unmutes media elements and clicks a control whose
label reads "Enable Sound", "Unmute", "Tap for sound" or the like, once,
retrying for a minute while players appear. Superimposed pages are left alone:
there the mixer plays the media itself.

