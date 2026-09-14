# Install on macOS

Open the disk image, drag the app to Applications, right click it the first
time. The media stack travels inside the app, so there is nothing else to
install.

* [Which file to download](#which-file-to-download)
* [The Gatekeeper warning](#the-gatekeeper-warning)
* [What is inside the app](#what-is-inside-the-app)
* [Where your config and your logs live](#where-your-config-and-your-logs-live)
* [Pointing the app at a mixer on another machine](#pointing-the-app-at-a-mixer-on-another-machine)
* [Running the mixer without the desktop app](#running-the-mixer-without-the-desktop-app)
* [When it does not start](#when-it-does-not-start)

## Which file to download

From the [releases page](https://github.com/psmux/GodwinMix/releases):

| File | What it does |
|---|---|
| `GodwinMix_<version>_aarch64.dmg` | the app, for Apple silicon |
| `GodwinMix-<version>-macos.app.tar.gz` | the same app, as an archive |
| `godwinmix-<version>-aarch64-apple-darwin.tar.gz` | the mixer and `gmx` on their own, no app, no GStreamer |

Check what you downloaded against `SHA256SUMS` in the same release:

```sh
shasum -a 256 GodwinMix_0.2.0_aarch64.dmg
```

## The Gatekeeper warning

The app is not signed with an Apple Developer ID and not notarised, so macOS
refuses to open it on a double click: "GodwinMix cannot be opened because the
developer cannot be verified."

The way past it, once:

1. Find GodwinMix in Applications.
2. Control click it, and choose **Open**.
3. The same warning appears with an **Open** button on it this time. Click it.

macOS remembers, and every later launch is an ordinary double click.

If the dialog gives you no Open button at all, the file carries a quarantine
flag from the browser that downloaded it. Clear it:

```sh
xattr -dr com.apple.quarantine /Applications/GodwinMix.app
```

Run that only on a file whose SHA256 you have checked against the release.

None of this is macOS finding something wrong with the app. It is macOS saying
it does not know who built it, which is true until the project pays for a
Developer ID and a notarisation run. `bundle.macOS.signingIdentity` is
deliberately absent from `tauri.conf.json`; a release job will add it with its
own secrets.

## What is inside the app

The whole media stack. `GodwinMix.app/Contents/Resources/gstreamer/macos` is a
GStreamer trimmed to what the codec catalogue and the pipelines actually name,
with its library paths rewritten so it runs wherever the app is put.

| Inside | Roughly |
|---|---|
| the desktop shell | 9 MB |
| the mixer itself | 15 MB |
| GStreamer, trimmed | 84 MB |

The app never uses a GStreamer installed elsewhere on the machine, even if you
have one from Homebrew. It pins `GST_PLUGIN_SYSTEM_PATH` to its own directory
before it starts the mixer, because a mixed 1.26 and 1.28 plugin set is a
crash nobody can read the backtrace of.

To see that for yourself, with no screen:

```sh
/Applications/GodwinMix.app/Contents/MacOS/godwinmix-desktop --headless-check
```

It prints where the bundled GStreamer is, which file `compositor`, the
software H.264 encoder, `rtmp2sink` and `srtsink` each came out of, starts the
mixer, asks it what it is, stops it and checks the port went quiet. It exits 0
only if every step held.

VideoToolbox is in the bundle, so hardware H.264 and HEVC encode work on any
Mac made this decade. `godwinmix --probe` says which entry it picked.

## Where your config and your logs live

| What | Where |
|---|---|
| `godwinmix.toml`, your config | `~/Library/Application Support/mix.godwin.desktop` |
| `core-token`, the token the local mixer is started with | the same folder |
| `gstreamer-registry.bin`, GStreamer's plugin cache | the same folder |
| `mixer.log` and `mixer.log.1` | `~/Library/Logs/mix.godwin.desktop` |

**Open config folder** and **Open logs folder** in the app's menu go straight
there. `~/Library` is hidden in Finder; Go, Go to Folder, and paste the path.

The config folder is yours to edit. The app writes `godwinmix.toml` once, from
the commented example, and never writes over it again.

## Pointing the app at a mixer on another machine

The same app drives a mixer running headless on a server, and it is the same
operator UI either way.

1. Open the app. If it is already connected to the mixer on this computer,
   choose **Connect** from the menu.
2. Pick **Another machine**.
3. Give it an address: `studio.local:8080`, or a full
   `https://mix.example.org` if it is behind a reverse proxy.
4. Give it the token that mixer's config sets, if it has one.

The app never starts and never stops a mixer it did not start. Quitting leaves
a remote mixer exactly as it found it. The title bar says which mixer you are
looking at and which version it reported.

## Running the mixer without the desktop app

The `.tar.gz` from the release holds `godwinmix` and `gmx`, the same program
under two names, plus the commented example config. It does **not** carry
GStreamer. Two ways to give it one:

**Homebrew**, which is the shortest path on a development machine:

```sh
brew install gstreamer
./godwinmix --config ./godwinmix.toml
```

**Use the app's copy**, if the desktop app is installed:

```sh
gst=/Applications/GodwinMix.app/Contents/Resources/gstreamer/macos
export GST_PLUGIN_PATH="$gst/lib/gstreamer-1.0"
export GST_PLUGIN_SYSTEM_PATH="$GST_PLUGIN_PATH"
export GST_PLUGIN_SCANNER="$gst/libexec/gstreamer-1.0/gst-plugin-scanner"
./godwinmix --config ./godwinmix.toml
```

There is no `DYLD_LIBRARY_PATH` in that second recipe on purpose. The bundled
libraries record their own locations relative to the files that load them, so
they are found without it, and `DYLD_*` variables are stripped by the system
for a signed binary anyway.

Either way, check the machine before you point cameras at it:

```sh
gmx doctor
./godwinmix --probe
```

## When it does not start

**"The developer cannot be verified".** See
[the Gatekeeper warning](#the-gatekeeper-warning) above.

**"The mixer did not answer within 25 seconds".** The first launch builds
GStreamer's plugin registry. Try again: the second start uses the cache in
`~/Library/Application Support/mix.godwin.desktop/gstreamer-registry.bin`. If
it happens every time, delete that file and read `mixer.log`.

**"The mixer started and stopped again".** Its own reason is the last thing in
`~/Library/Logs/mix.godwin.desktop/mixer.log`.

**No camera in the list.** macOS asks for camera and microphone permission per
app, and it asks the first time something tries to open one. System Settings,
Privacy and Security, Camera. The app that has to be allowed is GodwinMix,
even though it is the mixer inside it doing the capture.

**A screen capture source shows a black rectangle.** Screen Recording is a
separate permission in the same Privacy and Security list, and it takes effect
only after the app is quit and reopened.

**Two mixers.** If the app is killed rather than quit, the mixer it started
keeps running, which is the right way round: a broadcast should not end
because a window was force quit. The next launch finds it again and connects
to it rather than starting a second one.

**Something else.** `gmx doctor`, or the log. Then
[Debug a show](debug-a-show.md).

## See also

* [The desktop app](desktop-app.md), how it is built and what the first launch creates
* [Cross platform differences](../explanation/cross-platform.md), what is gated where and why
* [Run it on a headless server](headless-server.md)
* [Install on Windows](install-on-windows.md), [Install on Linux](install-on-linux.md)
