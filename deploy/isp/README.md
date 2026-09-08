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
| mediamtx | RTMP in, HLS out | programme at https://stream.spinber.com/live/program/ |
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
* wpesrc needs an EGL context and the mixer's GL elements create theirs first,
  as GLX under X11: `GST_GL_PLATFORM=egl` for the wpesrc mixer.
* The quiz server bound to `"localhost"`, which Node 20 resolves to `::1`
  only; it binds `0.0.0.0` now (or `HOST`).
