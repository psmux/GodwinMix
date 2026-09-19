# Install on Windows

No public installers are published yet. Use the source build instructions in
[CONTRIBUTING.md](../../CONTRIBUTING.md) for now. The installer instructions
below apply once a validated release is available.

Download the installer, run it, click past the warning, open the app. There is
nothing else to install: the media stack travels inside the app.

* [Which file to download](#which-file-to-download)
* [The SmartScreen warning](#the-smartscreen-warning)
* [What is inside the installer](#what-is-inside-the-installer)
* [Where your config and your logs live](#where-your-config-and-your-logs-live)
* [Pointing the app at a mixer on another machine](#pointing-the-app-at-a-mixer-on-another-machine)
* [Running the mixer without the desktop app](#running-the-mixer-without-the-desktop-app)
* [When it does not start](#when-it-does-not-start)

## Which file to download

From the [releases page](https://github.com/psmux/GodwinMix/releases), pick one:

| File | What it does |
|---|---|
| `GodwinMix_<version>_x64_en-US.msi` | installs for everyone on the machine; wants an administrator |
| `GodwinMix_<version>_x64-setup.exe` | the same thing through an NSIS installer |
| `godwinmix-<version>-x86_64-pc-windows-msvc.zip` | the mixer and `gmx` on their own, no app, no GStreamer |

Either installer gives you the same app. Use the `.msi` if your workplace
deploys software with Group Policy, and the `.exe` otherwise.

Check what you downloaded against `SHA256SUMS` in the same release:

```powershell
Get-FileHash .\GodwinMix_0.2.0_x64_en-US.msi -Algorithm SHA256
```

## The SmartScreen warning

The installers are not signed with a code signing certificate, so Windows
shows a blue box saying "Windows protected your PC". Click **More info**, then
**Run anyway**.

This is not Windows finding something wrong with the file. It is Windows
saying it has not seen this publisher before, which is true, and which stays
true until the project buys an EV certificate. The honest way to check the
file is the hash above, against the one in the release.

The same warning appears on the first launch of the app itself if you
downloaded the `.exe`. Windows remembers the answer.

## What is inside the installer

The whole media stack, which is the point. GStreamer's own Windows runtime is
a 527 MB separate download, and nobody should have to install it before a
video mixer will start. So the installer carries a trimmed copy:

| Inside | Roughly |
|---|---|
| the desktop shell | 9 MB |
| the mixer itself | 8 MB |
| GStreamer, trimmed to what the codec catalogue and the pipelines name | 85 to 110 MB |
| the camera, the screen and the microphone, which are plugins | 4 MB |

Those three are installed for you. The app copies them into
`%APPDATA%\mix.godwin.desktop\plugins` the first time it starts the mixer, so
a camera is in the add source list from the first launch and nothing has to be
typed at a command prompt. Anything you add later from the window goes into the
same folder and survives an update.

The budget for the whole installer is 150 MB, measured on every push by the
`platforms` job in CI, which fails if it is exceeded. OBS Studio, for
comparison, ships about 160 MB with everything inside it.

What travels: the core and base plugins, the good plugins the containers and
the multiview mosaic need, `rtmp2` and `srt` for the two ways a programme
leaves the building, libav for AAC, the WebRTC plugins, and Media Foundation
and Direct3D 11 for hardware encode and decode. What does not: every
development file, the MinGW tree, the Python and Perl bindings, the editing
services, and every plugin for a format no entry in `codecs.toml` can select.

The trimmed tree is built by `dev\bundle-gstreamer.ps1`, and the rule for what
travels is the codec catalogue, not a list somebody maintains by hand.

The app never uses a GStreamer installed elsewhere on the machine. It pins
`GST_PLUGIN_SYSTEM_PATH` to its own directory before it starts the mixer,
because a mixed 1.26 and 1.28 plugin set is a crash nobody can read the
backtrace of.

To see that for yourself, with no screen and no clicking:

```powershell
& "$env:ProgramFiles\GodwinMix\godwinmix-desktop.exe" --headless-check
```

It prints where the bundled GStreamer is, which file `compositor`, the
software H.264 encoder, `rtmp2sink` and `srtsink` each came out of, starts the
mixer, asks it what it is, stops it and checks the port went quiet. It exits 0
only if every step held.

## Where your config and your logs live

| What | Where |
|---|---|
| `godwinmix.toml`, your config | `%APPDATA%\mix.godwin.desktop` |
| `core-token`, the token the local mixer is started with | the same folder |
| `gstreamer-registry.bin`, GStreamer's plugin cache | the same folder |
| `mixer.log` and `mixer.log.1` | `%LOCALAPPDATA%\mix.godwin.desktop\logs` |

**Open config folder** and **Open logs folder** in the app's menu go straight
there. Paste either path into Explorer's address bar to get there by hand.

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

To go back to the mixer on this computer, choose **Connect** again and pick
**This computer**.

## Running the mixer without the desktop app

The `.zip` from the release holds `godwinmix.exe` and `gmx.exe`, the same
program under two names, plus the commented example config. It does **not**
carry GStreamer. Two ways to give it one:

**Use the app's copy.** If the desktop app is installed, its runtime is
already on the machine:

```powershell
$gst = "$env:ProgramFiles\GodwinMix\gstreamer\windows"
$env:GST_PLUGIN_PATH = "$gst\lib\gstreamer-1.0"
$env:GST_PLUGIN_SYSTEM_PATH = $env:GST_PLUGIN_PATH
$env:GST_PLUGIN_SCANNER = "$gst\libexec\gstreamer-1.0\gst-plugin-scanner.exe"
$env:PATH = "$gst\bin;$env:PATH"
.\godwinmix.exe --config .\godwinmix.toml
```

**Install GStreamer yourself.** Take the MSVC **runtime** installer from
[gstreamer.freedesktop.org](https://gstreamer.freedesktop.org/download/), the
complete profile, and add `C:\gstreamer\1.0\msvc_x86_64\bin` to `PATH`. You
want the development installer too only if you are going to build the mixer
from source.

Either way, check the machine before you point cameras at it:

```powershell
.\gmx.exe doctor
.\godwinmix.exe --probe
```

`doctor` names anything missing and which package it comes from. `--probe`
prints which encoder it would pick here.

## When it does not start

**"Windows protected your PC".** See
[the SmartScreen warning](#the-smartscreen-warning) above.

**The window opens and says the mixer did not answer within 25 seconds.** The
first launch builds GStreamer's plugin registry, which on a slow disk takes
longer than that once. Try again: the second start uses the cache in
`%APPDATA%\mix.godwin.desktop\gstreamer-registry.bin`. If it happens every
time, delete that file and look at `mixer.log`.

**"The mixer started and stopped again".** Its own reason is the last thing in
`mixer.log`. Open the logs folder from the menu.

**No camera in the list.** Windows asks for camera permission per app.
Settings, Privacy and security, Camera, and turn on "Let desktop apps access
your camera". GodwinMix uses `mfvideosrc` and falls back to `ksvideosrc`,
because `mfvideosrc` has an open upstream bug where it fails to start on some
devices ([gstreamer#2748](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/2748)).

**The audio stutters.** `wasapi2sink` has two open upstream bugs
([#2870](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/2870) and
[#3339](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/3339)).
The capture plugin falls back to `directsoundsink`, which is older and
steadier. [Cross platform differences](../explanation/cross-platform.md) lists
what is gated where.

**Two mixers.** If the app is killed rather than quit, the mixer it started
keeps running, which is the right way round: a broadcast should not end
because a window was force quit. The next launch finds it again on the port in
`local-core.port` and connects to it. Delete that file if you want a fresh
mixer and do not mind the old one carrying on.

**Something else.** `gmx doctor` from the zip, or the log. Then
[Debug a show](debug-a-show.md).

## See also

* [The desktop app](desktop-app.md), how it is built and what the first launch creates
* [Cross platform differences](../explanation/cross-platform.md), what is gated where and why
* [Run it on a headless server](headless-server.md)
* [Install on macOS](install-on-macos.md), [Install on Linux](install-on-linux.md)
