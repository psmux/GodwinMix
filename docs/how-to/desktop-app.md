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
* Windows and GStreamer: [GStreamer on Windows](#gstreamer-on-windows)
* Updates: [Updates](#updates)

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

**2. Bundle.**

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

Measured on an Apple M4 Pro, version 0.2.0, with no GStreamer bundled:

| Artefact | Size |
|---|---|
| `GodwinMix.app` | 17 MB (8.8 MB shell, 8.2 MB mixer) |
| `GodwinMix_0.2.0_aarch64.dmg` | 7.2 MB |

The budget for a Windows installer with GStreamer inside it is 150 MB
(09-builders). The shell and the mixer together are 17 MB of that, which
leaves the media stack about 130 MB to fit in.

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
`/api/status` instead, and the app says "an older version" rather than refusing
to connect.

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

## GStreamer on Windows

This is the one platform where the media stack cannot be left to the machine.
GStreamer's own Windows runtime installer is 527 MB, it is a separate download,
and asking a volunteer with four hours to install it before the app will start
is the same as telling them to use something else. OBS ships 160 MB with
everything inside it, and the budget here is 150 MB total.

**What the shell already does.** Before it starts the mixer, the shell looks
for a `gstreamer` directory in the app's resources. If it is there, the mixer
is started with:

| Variable | Set to |
|---|---|
| `GST_PLUGIN_PATH` | `gstreamer/lib/gstreamer-1.0`, or `lib64/gstreamer-1.0`, or `plugins`, whichever exists |
| `GST_PLUGIN_SYSTEM_PATH` | the same directory, so a GStreamer installed on the machine is not loaded alongside this one; a mixed 1.26 and 1.28 plugin set is a crash nobody can read |
| `GST_PLUGIN_SCANNER` | `gst-plugin-scanner` from `libexec/gstreamer-1.0`, when it is there |
| `PATH` | `gstreamer/bin` first, then what was already there, which is how Windows finds the DLLs |
| `DYLD_LIBRARY_PATH` or `LD_LIBRARY_PATH` | `gstreamer/lib`, on macOS and Linux |
| `GST_REGISTRY` | the plugin cache, in the application data directory, because the app's own directory is read only once it is installed |

With no such directory, none of this is set and the mixer uses the GStreamer on
the machine, which is what happens on macOS and Linux today. So dropping a
trimmed runtime into `tauri-app/gstreamer/` and adding one line to
`tauri.conf.json` is the whole of the remaining work. No code changes.

**What to put in it.** From the MSVC runtime MSI (not the development one,
which is the larger half of the 527 MB, and not MinGW):

| Take | Why |
|---|---|
| `gstreamer-1.0`, `glib-2.0`, `gobject`, `gio`, `orc` core DLLs | nothing runs without them |
| `gstcoreelements`, `gsttypefindfunctions`, `gstapp` | queues, tees, capsfilters, appsrc and appsink |
| base: `gstvideoconvertscale`, `gstaudioconvert`, `gstaudioresample`, `gstcompositor`, `gstaudiomixer`, `gstplayback`, `gsttcp`, `gstvideotestsrc`, `gstaudiotestsrc` | the canvas, the mix and the slate |
| good: `gstisomp4`, `gstmatroska`, `gstflv`, `gstrtp`, `gstrtsp`, `gstudp`, `gstjpeg`, `gstautodetect` | containers and the multiview mosaic |
| bad: `gstrtmp2`, `gstsrt`, `gstmpegtsmux`, `gstfdkaac` if it is there, `gstwebrtc` and `gstdtls` when WHEP is on | RTMP out is `rtmp2sink`; SRT is `srtsink` |
| libav: `gstlibav` and the FFmpeg DLLs it needs | the AAC encoder and the decoders with no better native option |
| Media Foundation and D3D11: `gstmediafoundation`, `gstd3d11` | hardware encode and decode on Windows, which is the point of being on Windows |
| `gst-plugin-scanner.exe` | the registry cannot be built without it |

Leave out: every `-devel` file (headers, `.lib`, `.pc`), the Python and Perl
bindings, `gst-devtools`, the editing services, the examples, the MinGW tree,
and every plugin for a format the codec catalogue does not name. The rule for
deciding is the codec catalogue, `codecs.toml`: if no entry can select an
element from a plugin, that plugin does not travel.

Then, in `tauri.conf.json`:

```json
"bundle": {
  "resources": { "gstreamer/": "gstreamer/" }
}
```

and the shell finds it at `resource_dir()/gstreamer` on every platform.

Two numbers have to be measured before this is called done: the size of the
trimmed tree, against the 130 MB the shell and the mixer leave of the 150 MB
budget, and the time from a cold start to `control server listening` with the
registry cache empty, which is the slowest thing a first launch does.

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
