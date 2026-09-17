# Install on Linux

Three ways in, depending on what you are doing: the `.deb` on a desktop that
runs Debian or Ubuntu, the AppImage on anything else, and the plain binary or
the container on a server with no screen.

* [Which file to download](#which-file-to-download)
* [The .deb](#the-deb)
* [The AppImage](#the-appimage)
* [The server, with no desktop at all](#the-server-with-no-desktop-at-all)
* [Where your config and your logs live](#where-your-config-and-your-logs-live)
* [Pointing the app at a mixer on another machine](#pointing-the-app-at-a-mixer-on-another-machine)
* [When it does not start](#when-it-does-not-start)

## Which file to download

From the [releases page](https://github.com/psmux/GodwinMix/releases):

| File | For |
|---|---|
| `godwinmix_<version>_amd64.deb` | Debian, Ubuntu and their derivatives |
| `godwinmix_<version>_amd64.AppImage` | Fedora, Arch, openSUSE, anything else |
| `godwinmix-<version>-x86_64-unknown-linux-gnu.tar.gz` | a server: the mixer and `gmx`, no desktop app |
| `godwinmix-<version>-aarch64-unknown-linux-gnu.tar.gz` | a Raspberry Pi or an arm64 server |
| `ghcr.io/psmux/godwinmix:<version>` | a container, which carries its own GStreamer |

Check what you downloaded against `SHA256SUMS` in the same release:

```sh
sha256sum -c SHA256SUMS --ignore-missing
```

Linux has no Gatekeeper and no SmartScreen, so nothing warns you and nothing
has to be clicked past. That cuts both ways: check the hash.

## The .deb

```sh
sudo apt install ./godwinmix_0.2.0_amd64.deb
```

Unlike the Windows and macOS builds, the `.deb` does **not** carry its own
GStreamer. It depends on the distribution's packages, which `apt` pulls in
with it:

```
libgstreamer1.0-0, libgstreamer-plugins-base1.0-0, gstreamer1.0-plugins-base,
gstreamer1.0-plugins-good, gstreamer1.0-plugins-bad, gstreamer1.0-libav
```

That is the right trade on a distribution that already has a packaged
GStreamer: the security updates are the distribution's job, the download is 20
MB rather than 110, and the plugins on the machine are the ones the machine's
own applications use. It is also why the `.deb` needs no permission dialog and
no unpacking step.

What the package does carry is the camera, the screen and the microphone.
Those three are GodwinMix plugins rather than GStreamer plugins, they add
about 4 MB, and the app copies them into
`~/.local/share/mix.godwin.desktop/plugins` the first time it starts the
mixer. So a webcam is in the add source list from the first launch with
nothing to install and nothing to type.

Two packages `apt` will not install for you, worth adding by hand:

```sh
# hardware encode on Intel and AMD
sudo apt install gstreamer1.0-vaapi va-driver-all
# x264, which is GPL, and the AAC encoder
sudo apt install gstreamer1.0-plugins-ugly
```

Then check the machine before you point cameras at it:

```sh
gmx doctor
godwinmix --probe
```

`doctor` names anything missing and the package it comes from. `--probe`
prints which encoder it picked.

## The AppImage

```sh
chmod +x godwinmix_0.2.0_amd64.AppImage
./godwinmix_0.2.0_amd64.AppImage
```

The AppImage does carry its own GStreamer, trimmed to what the codec catalogue
and the pipelines name, which is why it is about 110 MB against the `.deb`'s
20. That is the point of an AppImage: one file, no package manager, the same
plugin set on every distribution.

Some distributions no longer ship the FUSE 2 library an AppImage needs. If you
get "dlopen(): error loading libfuse.so.2":

```sh
# Debian and Ubuntu
sudo apt install libfuse2
# Fedora
sudo dnf install fuse-libs
```

Or extract it and run it from the directory, which needs no FUSE at all:

```sh
./godwinmix_0.2.0_amd64.AppImage --appimage-extract
./squashfs-root/AppRun
```

Hardware encode is the one thing an AppImage cannot bring with it: VA-API talks
to the kernel driver through a `.so` that has to match your graphics stack, so
the AppImage uses the one on the machine. Install `va-driver-all` (Debian) or
`libva-intel-driver` / `mesa-va-drivers` (Fedora) and `godwinmix --probe` will
find it.

## The server, with no desktop at all

This is the path most installations take, and the desktop app is not part of
it. Full instructions are in
[Run it on a headless server](headless-server.md); the short version:

```sh
sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-libav gstreamer1.0-tools
tar xzf godwinmix-0.2.0-x86_64-unknown-linux-gnu.tar.gz
cd godwinmix-0.2.0-x86_64-unknown-linux-gnu
./godwinmix --example-config > godwinmix.toml
# put a token in it, then
./godwinmix --config godwinmix.toml
```

The archive also carries `deploy/systemd/godwinmix.service` for running it as a
service. Or skip all of it and use the container, which has its GStreamer
inside:

```sh
docker run -d --name godwinmix -p 127.0.0.1:8080:8080 \
  -e GODWINMIX_TOKEN=change-me ghcr.io/psmux/godwinmix:latest
```

A desktop app on your laptop then connects to it over the network. See
[pointing the app at a mixer on another machine](#pointing-the-app-at-a-mixer-on-another-machine).

## Where your config and your logs live

For the desktop app:

| What | Where |
|---|---|
| `godwinmix.toml`, your config | `~/.local/share/mix.godwin.desktop` |
| `core-token`, the token the local mixer is started with | the same folder |
| `gstreamer-registry.bin`, GStreamer's plugin cache | the same folder |
| `mixer.log` and `mixer.log.1` | `~/.local/share/mix.godwin.desktop/logs` |

**Open config folder** and **Open logs folder** in the app's menu go straight
there.

For a mixer you started yourself, the config is wherever you pointed `--config`
and the log is whatever you redirected stderr to. Under systemd it is
`journalctl -u godwinmix`.

The AppImage's `--headless-check` proves the bundled runtime is the one being
loaded, the same way the Windows and macOS builds do:

```sh
./squashfs-root/AppRun --headless-check
```

## Pointing the app at a mixer on another machine

1. Open the app. If it is already connected to the mixer on this computer,
   choose **Connect** from the menu.
2. Pick **Another machine**.
3. Give it an address: `studio.local:8080`, or a full
   `https://mix.example.org` if it is behind a reverse proxy.
4. Give it the token that mixer's config sets.

The app never starts and never stops a mixer it did not start. Quitting leaves
a remote mixer exactly as it found it.

## When it does not start

**The `.deb` installs and the app opens on a blank window.** The webview comes
from WebKitGTK, which the `.deb` depends on but which some minimal desktops do
not pull in fully. `sudo apt install libwebkit2gtk-4.1-0`. On a machine with
no GPU driver, `WEBKIT_DISABLE_COMPOSITING_MODE=1` before the app is the usual
fix.

**"The mixer did not answer within 25 seconds".** The first launch builds
GStreamer's plugin registry. Try again; the second start uses the cache. If it
happens every time, delete `gstreamer-registry.bin` from the config folder and
read `mixer.log`.

**No camera in the list.** `ls /dev/video*`. If the devices are there but not
readable, your user is not in the `video` group:
`sudo usermod -aG video $USER`, then log out and back in.

**No audio.** GodwinMix uses PulseAudio or PipeWire through `pulsesrc`, and
falls back to `alsasrc`. `gst-device-monitor-1.0 Audio` lists what GStreamer
can see, which is the list the mixer sees too.

**Hardware encode is not picked.** `godwinmix --probe` says what it chose and
why. `vainfo` says whether VA-API works at all on this machine; if that fails,
the mixer is right to use the software encoder.

**Wayland and screen capture.** `ximagesrc` only works under X11. On Wayland
the screen capture path goes through PipeWire and the desktop portal, which
asks you which window to share the first time.

**Something else.** `gmx doctor`, or the log. Then
[Debug a show](debug-a-show.md).

## See also

* [Run it on a headless server](headless-server.md)
* [Run it on a Raspberry Pi](run-on-a-raspberry-pi.md)
* [The desktop app](desktop-app.md)
* [Cross platform differences](../explanation/cross-platform.md), what is gated where and why
* [Install on Windows](install-on-windows.md), [Install on macOS](install-on-macos.md)
