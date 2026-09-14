# The desktop app

GodwinMix ships as a desktop app on Windows, macOS and Linux. The app is a
window, a tray icon and a menu. Everything the operator actually looks at is
served by the mixer, so the window shows the same page whether the mixer is on
this computer or on a server in another building.

The mixer itself travels inside the app as a sidecar binary. On first launch
the app writes a config file, generates a token, takes a free port from the
operating system and starts the mixer on it. There is no port and no token
compiled into the shell.

* Building it: [Building](#building)
* What the first launch creates: [First launch](#first-launch)
* Driving a mixer on another machine: [Connecting to a server](#connecting-to-a-server)
* The bundled media stack: [The GStreamer inside the app](#the-gstreamer-inside-the-app)
* Updates: [Updates](#updates)

Installing a release rather than building one is
[Windows](install-on-windows.md), [macOS](install-on-macos.md) and
[Linux](install-on-linux.md). What differs between the three, and why, is
[Cross platform](../explanation/cross-platform.md).

## Building

You need the Rust toolchain, the Tauri CLI (`cargo install tauri-cli --version
"^2"`), and whatever the mixer itself needs on your platform (see the platform
table in the README). Then two steps, in order.

**1. Put the mixer where the bundler looks for it.** Tauri copies external
binaries by target triple, so the file has to be named for the machine it was
built on:

```sh
cargo build --release --bin godwinmix
cp target/release/godwinmix \
   tauri-app/binaries/godwinmix-$(rustc -vV | sed -n 's/^host: //p')
```

On Windows the name ends in `.exe`:
`tauri-app\binaries\godwinmix-x86_64-pc-windows-msvc.exe`.

These files are build output, not source, and `tauri-app/binaries/.gitignore`
keeps them out of the repository. CI builds the mixer for each platform and
copies it here before the next step.

**2. Put a GStreamer where the bundler looks for it**, if this build is going
to run on a machine that has none:

```sh
dev/bundle-gstreamer.sh
```

That writes `tauri-app/gstreamer/<platform>/` and `tauri.conf.json` carries it
into the bundle. Skipping this step is fine for a developer build: the app
then uses the GStreamer on the machine, which is what the Linux `.deb` does on
purpose. See [The GStreamer inside the app](#the-gstreamer-inside-the-app).

**3. Bundle.**

```sh
cd tauri-app
cargo tauri build
```

What comes out, per platform:

| Platform | Artefacts | Where |
|---|---|---|
| macOS | `GodwinMix.app`, `GodwinMix_<version>_<arch>.dmg` | `tauri-app/target/release/bundle/macos/`, `.../dmg/` |
| Windows | `.msi` (WiX), `.exe` (NSIS, per machine install) | `tauri-app/target/release/bundle/msi/`, `.../nsis/` |
| Linux | `.deb`, `.AppImage` | `tauri-app/target/release/bundle/deb/`, `.../appimage/` |

Measured on an Apple M4 Pro, version 0.2.0:

| Artefact | Size |
|---|---|
| `GodwinMix.app`, no GStreamer | 17 MB (8.8 MB shell, 8.2 MB mixer) |
| `GodwinMix_0.2.0_aarch64.dmg`, no GStreamer | 7.2 MB |
| `GodwinMix.app`, GStreamer inside it | 108 MB (8.8 MB shell, 14 MB mixer, 85 MB GStreamer) |

The mixer grew from 8.2 MB to 14 MB between those two measurements, for
reasons that have nothing to do with this page. The number that matters is the
85 MB: the budget for a Windows installer with GStreamer inside it is 150 MB,
the shell and the mixer are about 23 MB of that, and the trimmer refuses to
finish over 130 MB.

Build one kind at a time with `--bundles app`, `--bundles dmg`, `--bundles
msi`, and so on. The `.dmg` step asks Finder to arrange the disk image window,
through AppleScript, and fails with `AppleEvent timed out (-1712)` on a machine
where the terminal has not been given automation permission. Add
`--skip-jenkins` when driving `bundle_dmg.sh` by hand, or grant the permission;
the image itself is fine either way.

Nothing is signed or notarised. `bundle.macOS.signingIdentity` and the Windows
certificate settings are deliberately absent: a release job adds them with its
own secrets.

## First launch

The app opens on its own connect page, asks where the mixer should be, and
remembers the answer. Choosing "This computer" makes, in the application data
directory:

| File | What it is |
|---|---|
| `godwinmix.toml` | the mixer's config, written from `godwinmix.example.toml` with the example's sample cameras commented out, and yours to edit from then on |
| `core-token` | the bearer token the local mixer is started with, generated once, owner readable only |
| `connection.json` | which mixer was connected to last |
| `local-core.port` | the port the local mixer was given, so a shell that crashed finds its mixer again instead of starting a second one |
| `gstreamer-registry.bin` | GStreamer's plugin cache, kept here because an installed app's own directory is read only |

Where that directory is, and where the mixer's log goes:

| Platform | Application data | Logs |
|---|---|---|
| macOS | `~/Library/Application Support/mix.godwin.desktop` | `~/Library/Logs/mix.godwin.desktop` |
| Windows | `%APPDATA%\mix.godwin.desktop` | `%LOCALAPPDATA%\mix.godwin.desktop\logs` |
| Linux | `~/.local/share/mix.godwin.desktop` | `~/.local/share/mix.godwin.desktop/logs` |

**Open config folder** and **Open logs folder** in the menu go straight there.
Everything the mixer prints goes to `mixer.log`, with one previous file kept as
`mixer.log.1` once it passes 4 MB.

The port is not remembered between sessions on purpose. A new one is taken at
every start, so two copies of the app, or the app beside a mixer somebody
started from a terminal, never fight over 8080.

To check all of this without a screen, on your machine or in CI:

```sh
./tauri-app/target/release/bundle/macos/GodwinMix.app/Contents/MacOS/godwinmix-desktop --headless-check
```

It starts the mixer the way the app does, prints the port, the process id and
what the mixer says it is, waits three seconds, stops it, and confirms the port
went quiet. It exits 0 only if every step held.

## Connecting to a server

The same app drives a mixer running headless somewhere else. Pick "Another
machine" in the connect dialog, give it an address (`studio.local:8080`, or a
full `https://` URL) and the token that mixer's config sets, if it has one.

The app never starts and never stops a mixer it did not start. Quit leaves a
remote mixer exactly as it found it. "Quit and stop the mixer" asks it to stop
over the API and exits with status 2, which `dev/desktop.sh` reads as the cue
to stop the rest of the local test rig.

Closing the window does not quit: it hides. A mixer that went off air because
somebody tidied their desktop would be the worst kind of bug. The tray icon
brings the window back, and Quit is the way out.

The status line is the title bar: it names the version the mixer reported and
whether it is this computer or an address. The version comes from
`/api/v1/core/info`; a mixer built before that endpoint existed answers
`/api/status` instead, and the app says "(version not reported)" rather than
refusing to connect.

## The content security policy

`app.security.csp` in `tauri.conf.json` is no longer `null`. It applies to the
shell's own connect page, which is the only page the app itself serves:

```
default-src 'self'; img-src 'self' asset: data: blob:; media-src 'self' blob:;
style-src 'self'; script-src 'self'; connect-src 'self' ipc: http://ipc.localhost;
form-action 'none'; frame-ancestors 'none'; base-uri 'self'; object-src 'none'
```

It does not govern the operator UI. That page comes from the mixer over http,
so its policy is the `Content-Security-Policy` header the mixer sends, and
changing it means changing the mixer, not this file. Keep the two in step: the
connect page is deliberately the stricter of the two, because it is the page
that can talk to the shell.

Which it can, and the mixer's page cannot. `capabilities/desktop-shell.json`
lists what the shell's own page may ask for, and no remote origin is named
there or in `tauri.conf.json`, so a page served over http gets no IPC at all.
It cannot spawn the sidecar, read a path or ask for an update. The one channel
it has is a navigation to `godwinmix://quit` or `godwinmix://quit-all`, which
the shell answers and which carries nothing but its own name.
(`liveboxmix://` is answered too, until 0.3, for a page cached from before the
rename.)

## Updates

**Check for updates** in the menu calls the updater plugin, which is configured
and pointed at a placeholder:

```json
"updater": {
  "endpoints": ["https://releases.godwin.mix/desktop/{{target}}/{{arch}}/{{current_version}}"],
  "pubkey": "..."
}
```

Nothing is behind that address, so the honest answer today is "could not check
for updates", and that is what the app says. The public key in the config was
generated for the shape of the file; the matching private key was never kept,
and no signing key is enabled in this build. Before the first signed release:

1. `cargo tauri signer generate` on a machine that will hold the key, once.
2. Put the public key in `tauri.conf.json` and the private key and its password
   in the release job's secrets as `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Point `endpoints` at the real feed and set `"createUpdaterArtifacts": true`
   in `bundle`.

Until then the app is updated by downloading a new one.

## The GStreamer inside the app

This is the one thing the app cannot leave to the machine. GStreamer's own
Windows runtime installer is 527 MB, it is a separate download, and asking a
volunteer with four hours before a service to install it before the app will
start is the same as telling them to use something else. OBS ships 160 MB with
everything inside it, and the budget here is 150 MB total.

So the app carries a trimmed GStreamer, and the trimming is done by a script
rather than by hand.

### Building one

```sh
dev/bundle-gstreamer.sh                    # macOS and Linux
dev\bundle-gstreamer.ps1                   # Windows
```

Each fetches or finds the official runtime for the platform, trims it, writes
the result to `tauri-app/gstreamer/<platform>/`, prints the size, and refuses
to finish if the tree is over budget. Then it asks the trimmed tree for
`compositor`, `rtmp2sink`, `srtsink` and a software H.264 encoder, out of its
own registry with the system GStreamer shut out, because a tree of the right
size that does not load is worse than no tree at all.

Options worth knowing:

| Option | What it does |
|---|---|
| `--from <prefix>` / `-From` | trim a GStreamer already on the machine instead of downloading one |
| `--version 1.28.7` / `-Version` | download that release |
| `--budget-mb 130` / `-BudgetMb` | what the tree may weigh; the default is 130 |
| `--exclude-gpl` / `-ExcludeGpl` | leave out x264 and x265, so the build can go out under Apache 2.0 |

Where the runtime comes from, per platform: the official `.pkg` on macOS
(`pkgutil --expand-full`, nothing installed), the MSVC runtime MSI on Windows
(`msiexec /a`, an administrative unpack, no elevation and no registry), and
the distribution's own packages on Linux, because that is what an AppImage
should carry.

### What decides what travels

`codecs.toml`. The shared trimmer, `dev/gst_trim.py`, asks `gst-inspect` which
plugin each element the catalogue can select lives in, and keeps those. On top
of that it keeps the elements the pipelines build by name (a list in the
script, with the `grep` that produced it in the comment above it) and the
capture and hardware plugins for the platform. A codec added to the catalogue
travels without anybody editing the bundler.

The libraries are not a list at all. Every kept plugin and every kept binary is
read for its dynamic imports, and the closure of that is what gets copied. A
plugin that quietly grew a dependency brings it along; a library nothing
references is dropped. The readers are `otool -L` on macOS, `objdump -p` on
Linux, and a small PE import table walk on Windows, so no Visual Studio is
needed to measure an installer.

Left out: every development file (headers, `.lib`, `.pc`), the MinGW tree, the
Python and Perl bindings, `gst-devtools`, the editing services, and every
plugin for a format no entry in the catalogue can select.

On macOS the tree is made relocatable as the last step. Each file's recorded
library paths are rewritten to `@loader_path` and then the file is signed
again with an ad hoc signature, because editing a Mach-O breaks its signature
and Apple silicon refuses to load a library that claims to be signed and is
not. That is what lets the same tree work inside `/Applications` without any
`DYLD_LIBRARY_PATH`.

### Measured

Homebrew GStreamer 1.28.7 on an Apple M4 Pro, trimmed:

| | Plugins | Libraries | Size |
|---|---|---|---|
| the source prefix | 277 | everything in the cellar | 163 MB of plugins alone |
| the trimmed tree | 55 | 81 | 84.3 MB |

Against the 130 MB the 150 MB installer budget leaves once the shell and the
mixer have had their share.

### How the app finds it

`tauri.conf.json` lists it under `bundle.resources`:

```json
"resources": { "gstreamer": "gstreamer" }
```

and the shell looks for `resource_dir()/gstreamer/<platform>` first and
`resource_dir()/gstreamer` after it. A directory with no plugins in it does
not count as a runtime, which is what lets the repository keep
`tauri-app/gstreamer/` with only a `.gitignore` in it: a build with no bundled
runtime falls back to the GStreamer on the machine, which is how a developer
build works and how the Linux `.deb` is meant to work.

With a runtime found, the mixer is started with:

| Variable | Set to |
|---|---|
| `GST_PLUGIN_PATH` | `gstreamer/lib/gstreamer-1.0`, or `lib64/gstreamer-1.0`, or `plugins`, whichever exists |
| `GST_PLUGIN_SYSTEM_PATH` | the same directory, so a GStreamer installed on the machine is not loaded alongside this one; a mixed 1.26 and 1.28 plugin set is a crash nobody can read |
| `GST_PLUGIN_SCANNER` | `gst-plugin-scanner` from `libexec/gstreamer-1.0`, when it is there |
| `PATH` | `gstreamer/bin` first, then what was already there, which is how Windows finds the DLLs |
| `DYLD_LIBRARY_PATH` or `LD_LIBRARY_PATH` | `gstreamer/lib`, on macOS and Linux |
| `GST_REGISTRY` | the plugin cache, in the application data directory, because the app's own directory is read only once it is installed |

### Proving it is the one that loads

`--headless-check` does this before it starts the mixer. It runs the bundled
`gst-inspect-1.0`, in exactly the environment the mixer will be started in,
and asks where `compositor`, the software H.264 encoder, `rtmp2sink` and
`srtsink` each came from. Each answer has to be a file inside the bundle:

```
bundled GStreamer in /Applications/GodwinMix.app/Contents/Resources/gstreamer/macos
  compositor from .../lib/gstreamer-1.0/libgstcompositor.dylib
  rtmp2sink from .../lib/gstreamer-1.0/libgstrtmp2.dylib
  srtsink from .../lib/gstreamer-1.0/libgstsrt.dylib
  x264enc from .../lib/gstreamer-1.0/libgstx264.dylib
the bundled runtime answered for every element, and none came from a system install
```

An element that answered out of `/opt/homebrew` or `C:\Program Files\gstreamer`
fails the check by name, which is the failure that would otherwise only show
up on a machine with no GStreamer on it. A build with no bundled runtime skips
the section entirely and checks only the mixer.

### Licences

`gst-inspect` reports each plugin's licence and the trimmer prints the
copyleft ones it kept:

```
copyleft plugins in this tree: faad x264 x265
```

An installer carrying x264 is a GPL installer. `--exclude-gpl` drops those
plugins, and the catalogue falls back to openh264, which is exactly what its
`license` field is for. Whoever cuts a release decides; the script makes sure
nobody decides by accident.

One thing the flag cannot do on its own: it drops **plugins**, not libraries.
A libav built against `libx264`, which is what Homebrew and most
distributions ship, still carries it into the closure through
`libavcodec`. On the Homebrew tree, `--exclude-gpl` takes the tree from 84.3
MB to 83.8 MB and leaves `libavcodec` where it was. A genuinely licence clean
build needs a libav built without x264 as well, which means building FFmpeg,
and that is a decision for a release rather than a flag.

### What CI does with all this

`.github/workflows/platforms.yml` builds a trimmed runtime, bundles the app
with it, runs `--headless-check` and measures the installer, on all three
runners, on every push. A Windows installer over 150 MB fails the job.
`.github/workflows/release.yml` does the same before it publishes, so an
installer without a media stack inside it cannot be released.

## When it does not start

**"The mixer started and stopped again."** The mixer's own reason is in
`mixer.log`. The usual cause on a fresh machine is GStreamer missing or half
installed.

**"The mixer did not answer within 25 seconds."** A cold GStreamer registry
scan on a small board can take that long once. Try again; the second start uses
the cache.

**Two mixers.** If the shell is killed rather than quit, the mixer it started
keeps running, which is the right way round: a broadcast should not end because
a window was force quit. The next launch finds it on the port in
`local-core.port`, checks it answers with this machine's token, and connects to
it rather than starting a second one. Delete that file if you want a fresh
mixer and do not mind the old one carrying on.
