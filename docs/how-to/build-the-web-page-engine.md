# Build the web page engine with H.264 and AAC

Web page sources render through CEF, the Chromium Embedded Framework. The
official CEF builds leave out H.264 and AAC, so a YouTube live stream or any
H.264 only video says "Your browser can't play this video". GodwinMix ships
its own build of CEF instead, made by the scripts in
[psmux/godwinmix-cef](https://github.com/psmux/godwinmix-cef) with Chromium's
codec flags on. This page is for whoever looks after a release: how to make
that build for a new CEF version, and how to check it before it ships.

Our build exists for Windows (`windows64`) and Linux (`linux64`,
`linuxarm64`). There is no Mac build of ours yet, so the macOS app ships the
official CEF and does not play H.264 or AAC.

You need this when `browser/Cargo.toml` moves to a new `cef` crate, or when a
release stops with

```
GodwinMix's CEF 154.0.34 with H.264 and AAC has no linuxarm64 build yet
```

## Which version

The version is the `+metadata` of `cef-dll-sys` in `browser/Cargo.lock`:

```sh
grep -A1 '^name = "cef-dll-sys"$' browser/Cargo.lock
# version = "154.5.0+154.0.34"   means CEF 154.0.34
```

## Build it

On a machine of the platform you need (an Apple silicon Mac builds both Mac
platforms), with about 100 GB free and most of a day:

```sh
git clone https://github.com/psmux/godwinmix-cef.git
cd godwinmix-cef
./build.sh 154.0.34
```

The repository's README has the prerequisites per platform, the time and
disk each one took, and the settings (`JOBS` keeps some cores free,
`MIN_FREE_GB` stops it before the disk fills). If anything stops it, run the
same command again; it carries on from where it was.

## Check it before publishing

Build the renderer against the new archive and ask it what it can play. The
archive is in `~/cef-build/out/cef-154.0.34/` (`C:/cef-build/out/cef-154.0.34/`
on Windows):

```sh
browser/dev/install-cef-dist.sh ~/cef-build/out/cef-154.0.34/cef_binary_154.0.34+*_minimal.tar.bz2 /tmp/cef-new
cd browser && CEF_PATH=/tmp/cef-new cargo build --release
```

Then put a YouTube live stream in a web page source of a running mixer and
watch it: the picture moves, the source's meter shows sound, and
`gmx ctl rpc source.get '{"id":"<source>"}'` has no `page.notice`. The
official build fails that same test with "Your browser can't play this
video".

## Add macOS

On an Apple silicon Mac with Xcode, run `./build.sh 154.0.34`, then
`PLATFORM=macosx64 ./build.sh 154.0.34` for Intel Macs from the same
checkout, check both as above, and publish them. Then add `macosarm64
macosx64` to `CEF_CODECS_PLATFORMS` in `dev/cef-codecs.sh`, so a release
fails rather than quietly shipping the official CEF on a Mac.

## Publish it

```sh
./publish.sh 154.0.34
```

That uploads the archive with its `.sha1` and `.sha256` to the release
`cef-154.0.34` and rewrites the release's `index.json`. Run it on each
machine that built a platform, or copy the archives to one machine and name
them on the command line.

## What uses it

* `dev/bundle-browser.sh`, which the release workflow runs, points the `cef`
  crate at that release (`CEF_DOWNLOAD_URL`), lets the crate check the SHA-1,
  and checks the SHA-256 itself afterwards. A platform listed in
  `CEF_CODECS_PLATFORMS` in `dev/cef-codecs.sh` (today `windows64 linux64
  linuxarm64`) that is missing from the release stops it with the message
  above. Any other platform, which today means macOS, builds against the
  official CEF and prints a line saying so. `CEF_CODECS=off` builds against
  the official CEF everywhere, for a developer who does not need the codecs.
* A mixer run from a checkout sets web pages up by itself and looks in that
  release first, then at the official CDN for a platform the release does not
  have yet.

The [web page sources reference](../reference/web-page-sources.md#codecs) says
which codecs the renderer carries on each platform.
