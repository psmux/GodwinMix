# Cross platform: what is gated where, and why

Windows, macOS and Linux are all first class. That is a principle, not a hope,
and it costs something: every code path has to compile on all three, and the
parts that genuinely cannot be the same have to say so in the error message
rather than in a comment nobody reads.

This page is the honest account of where the three platforms differ, what the
fallback is in each case, and which upstream bugs the code is working around.

* [The rule](#the-rule)
* [The media contract: unixfd against the pipe](#the-media-contract-unixfd-against-the-pipe)
* [The sidecar output FIFO, and why Windows does not have one yet](#the-sidecar-output-fifo-and-why-windows-does-not-have-one-yet)
* [Audio and camera, per platform](#audio-and-camera-per-platform)
* [Hardware encode and decode](#hardware-encode-and-decode)
* [The bundled GStreamer](#the-bundled-gstreamer)
* [Process control](#process-control)
* [Everything else that is gated](#everything-else-that-is-gated)
* [Known upstream bugs](#known-upstream-bugs)
* [How this is kept honest](#how-this-is-kept-honest)

## The rule

Two sentences, and every gate on this page follows them.

**Every code path compiles on all three platforms.** A `#[cfg(unix)]` function
always has a `#[cfg(not(unix))]` twin with the same signature. The twin either
does the same thing another way, or returns a typed refusal. It never simply
vanishes, because a function that vanishes takes its caller with it and the
build breaks on a machine the author does not own.

**A refusal names the alternative.** "Not supported on Windows" is a dead end.
"a local raw preview needs a Unix socket and this core is running on Windows.
Use /mjpeg/program, which costs one JPEG encode per frame and works
everywhere" is a sentence somebody can act on. Every refusal in the tree is
written that way, and where one is not, that is a bug.

## The media contract: unixfd against the pipe

A plugin that runs beside the core has to get frames across a process
boundary. There are three transports and they are not equally available.

| Transport | Linux | macOS | Windows | What it costs |
|---|---|---|---|---|
| `unixfd` | yes | yes | no | nothing: the buffer is passed by descriptor, no copy |
| `shm` | yes | yes | no | nothing, once mapped |
| `container` | yes | yes | yes | one copy per frame, plus mux and demux |

`container` is a muxed stream on a pipe: the plugin writes to its own stdout
and the core reads it with `fdsrc`. It is the documented default for Windows
and it is what every plugin manifest should list unless it has a reason not
to.

The negotiation is in `crates/godwinmix-core/src/plugin/host/transport.rs`.
`usable()` is not `#[cfg]` gated: it uses a runtime `cfg!(unix)` so both arms
always compile, and on Windows it answers:

> the `unixfd` transport needs Unix sockets and this is windows. The plugin's
> manifest should list "container", which works everywhere.

`available()` filters the transport order through that, so on Windows the
negotiated list is `[container]` and nothing else. A plugin whose manifest
lists only socket transports is refused at the handshake, by name, before any
media flows.

On Windows the container path does not use `fdsrc`, because there is no
descriptor to hand it. `crates/godwinmix-core/src/input.rs` builds an `appsrc`
instead and a named reader thread pushes 4 MiB buffers into it
(`new_exec_source` and `attach_exec_stdout`, both with a `#[cfg(not(unix))]`
twin of the Unix version). That thread holds a revocation token in a static
map, so a restarted source cannot have its old reader push into the element
the new one now owns, or send it an end of stream.

The Windows shared memory transport the roadmap mentions is a later addition
behind the same transport list. Nothing about adding it changes a plugin: it
appears in `available()` and the handshake picks it.

## The sidecar output FIFO, and why Windows does not have one yet

An **output** plugin running as a sidecar is the one place the pipe is not
enough. The core has to push the programme *to* the plugin, so it makes a FIFO
with `mkfifo`, hands the path to the plugin in `GMX_MEDIA` and in the `start`
call, and writes to it with `filesink`.

Windows has no FIFO. `crates/godwinmix-core/src/plugin/host/output.rs` refuses
at `handshake`, before anything is spawned:

> an output plugin needs a FIFO to receive the programme on, and this is
> windows. Sidecar outputs are Unix only for now; a first party output
> (rtmp/output, srt/output) works everywhere.

So RTMP and SRT, which is every output most people use, work on Windows. A
third party output plugin does not, and that is the one real gap in the plugin
model on Windows today.

**What a Windows implementation has to do.** This is written down here rather
than left as "todo" because the work is bigger than it looks and the seams are
not where you would expect:

1. `make_fifo(path: &str) -> Result<()>` becomes a `CreateNamedPipeW` call. In
   the house style that is a local `#[link(name = "kernel32")] extern "system"`
   block, the way `observe/logs.rs` and `observe/doctor.rs` already do it, so
   no new crate. That part is thirty lines.
2. The **path shape changes**. `MediaDir::programme()` returns a filesystem
   path under the runtime directory; a Windows named pipe has to be
   `\\.\pipe\<name>`. So `transport.rs` needs a `cfg` split there, and
   `MediaDir`'s `Drop`, which today is a `remove_dir_all`, has to close a
   handle instead.
3. **`make_fifo` has to own something.** A FIFO is a file and the current
   signature returns `()`. A pipe server is a `HANDLE` that must be kept and
   closed, so the signature becomes `Result<PipeHandle>` and the handle is
   parked in `SidecarOutput`.
4. **`filesink` cannot serve a named pipe.** `build()` writes the programme
   with `filesink location=<fifo>`; on Windows that becomes an `appsink` and a
   writer thread onto the connected handle, which is exactly the reader thread
   in `input.rs` run backwards.
5. **Connection semantics differ.** Opening a FIFO blocks until the far end
   opens. A pipe server has to call `ConnectNamedPipe` and the plugin has to
   `CreateFile` the name, so there is an explicit accept step where today
   there is an ordering race that happens to be benign.
6. The SDK needs a matching opener. `media::open` routes `Transport::Container`
   to stdout; the output case is "container, but at this address", which the
   SDK does not model yet.

That is five files and well past a hundred and fifty lines, in two crates, one
of which is the plugin SDK other people build against. It is a change worth
designing rather than squeezing in, and `SidecarOutput` has no callers wired
up yet in any case, so nothing regresses by waiting.

## Audio and camera, per platform

The capture plugins pick the first element that exists and works, in this
order.

| | Camera | Audio in | Audio out |
|---|---|---|---|
| Linux | `v4l2src` | `pulsesrc`, then `alsasrc` | `pulsesink`, then `alsasink` |
| macOS | `avfvideosrc` | `osxaudiosrc` | `osxaudiosink` |
| Windows | `mfvideosrc`, then `ksvideosrc` | `wasapi2src`, then `wasapisrc` | `wasapi2sink`, then `directsoundsink` |

Two of the Windows fallbacks exist because of open upstream bugs rather than
because of old hardware. See [known upstream bugs](#known-upstream-bugs).

Screen capture is `ximagesrc` on X11, PipeWire and the desktop portal on
Wayland, `d3d11screencapturesrc` on Windows, and on macOS it goes through
Screen Recording permission, which only takes effect after the app is quit and
reopened.

## Hardware encode and decode

Every entry is in `codecs.toml` with a rank, and the probe picks the highest
ranked entry whose elements are actually installed. Nothing here is compiled
in, so a new GPU generation is an entry in a data file.

| Platform | What gets picked, best first |
|---|---|
| Linux | NVENC (`nvh264enc`), VA (`vah264enc`), Quick Sync (`qsvh264enc`), V4L2 on a Pi (`v4l2h264enc`), then x264 or openh264 |
| macOS | VideoToolbox (`vtenc_h264_hw`), then x264 or openh264 |
| Windows | NVENC, Quick Sync, AMF, Media Foundation (`mfh264enc`), then x264 or openh264 |

The software compositor stays the honest default everywhere. The GPU
compositors (`d3d11compositor`, `d3d12compositor`, `glvideomixer`,
`cudacompositor`, `vacompositor`) are ranked but promoted only after
`gmx codec test` and a soak, because several of them sit at rank none upstream
with open bugs and a D3D12 rebuild leak.

`godwinmix --probe` prints what this machine would pick, and `gmx doctor`
names anything missing and which package it comes from, in the words of the
platform you are standing in front of.

## The bundled GStreamer

| Platform | What ships | Why |
|---|---|---|
| Windows | a trimmed runtime inside the installer | the stock runtime installer is 527 MB and a separate download; nobody should have to do that before a mixer will start |
| macOS | a trimmed runtime inside the .app | the same reason, plus Homebrew is not a thing an operator has |
| Linux, .deb | nothing; the distribution's packages | the machine already has a packaged GStreamer and the security updates are the distribution's job |
| Linux, AppImage | a trimmed runtime | one file that runs on any distribution is the whole point of an AppImage |
| Linux, container | the image's own | same reasoning as the .deb |

The trimmed tree is built by `dev/bundle-gstreamer.sh` and
`dev/bundle-gstreamer.ps1`, which both call `dev/gst_trim.py`. The rule for
what travels is `codecs.toml`: every element an entry can select is looked up
in the registry, the plugin it lives in is kept, and everything else is left
behind. The libraries are not a list at all, they are the closure of what the
kept files actually import, read out of the Mach-O, ELF or PE headers.

Where a runtime is bundled, the shell pins `GST_PLUGIN_SYSTEM_PATH` to it
before starting the mixer, so a GStreamer installed on the machine is not
loaded alongside it. A mixed 1.26 and 1.28 plugin set is a crash nobody can
read the backtrace of. `--headless-check` proves the pinning worked by asking
the bundled `gst-inspect` where each element came from and failing if any of
them came from outside the bundle.

## Process control

Killing a source means killing whatever it spawned, and the three platforms do
not agree on how.

| | Unix | Windows |
|---|---|---|
| stop a child | `SIGTERM` to the process group, then `SIGKILL` | `Child::kill` |
| find grandchildren | `/proc/<pid>/task/<tid>/children` on Linux, nothing on macOS | nothing |
| keep grandchildren in the blast radius | `process_group(0)` at spawn | nothing, deliberately |
| is a pid alive | `kill(pid, 0)` | `tasklist /FI "PID eq <pid>"` |
| stall a process, for `gmx chaos` | `SIGSTOP` then `SIGCONT` | refused, by name |
| kill a process, for `gmx chaos` | `SIGKILL` | `taskkill /F` |
| resource sampling | `getrusage`, `/proc`, or `ps` | `tasklist` for memory; CPU is left unanswered rather than guessed |

Two Windows consequences are worth stating plainly because they are the kind
of thing that bites at three in the morning. An `--exec` source that spawns
children of its own can leave grandchildren behind on Windows, because the
equivalent guarantee needs a Job Object with `KILL_ON_JOB_CLOSE` and a handle
to own and close. And the stderr reader thread blocks in `read` rather than
waking to check a stop flag, because there is no `poll` on a pipe handle from
`std`; it still ends, because killing the child closes the pipe, but it ends
in the other order.

`gmx chaos stall` says so rather than pretending:

> stalling a process needs SIGSTOP, which Windows has no equivalent of that is
> safe to use here. `gmx chaos kill` works on every platform; the stall case is
> reproduced on Linux or macOS.

## Everything else that is gated

A complete list, so nobody has to grep for it.

| Where | Unix | Windows | Verdict |
|---|---|---|---|
| `preview/local.rs`, the raw local preview socket | `unixfdsink` on a Unix socket | refused, naming `/mjpeg/program` and `/whep/program` | the whole `imp` module is `cfg(unix)`; `unsupported_message()` compiles everywhere |
| `observe/logs.rs`, is stderr a terminal | `isatty` | `GetConsoleMode` on the standard error handle | both real; anything else gets JSON, which is the safe default |
| `observe/doctor.rs`, free disk and total memory | `statvfs`, `/proc/meminfo`, `sysctlbyname` | `GetDiskFreeSpaceExW`, `GlobalMemoryStatusEx` | both real, hand written `extern "system"`, no extra crate |
| `bench.rs`, CPU seconds and RSS | `getrusage`, `/proc/self/statm`, `ps` | one `Get-Process` call through PowerShell | a call rather than a dependency on `windows-sys` |
| `catalogue/check.rs`, the cost columns | `getrusage` | zero, and the report says the platform does not give them | a number nobody can trust is worse than a blank |
| `convert.rs`, thread priority | `setpriority` on Linux | no-op | the Linux per thread nice does not exist elsewhere |
| `plugin/loader.rs` and `cli/plugin.rs`, the executable bit | `PermissionsExt` | no-op | NTFS has no executable bit to carry |
| `godwinmix-tui/src/term.rs`, terminal picture support | asks the terminal and reads the raw reply | environment variables and `--picture` | reading a raw reply needs a tty in raw mode |
| `godwinmix-sdk`, `media/gst.rs` | `unixfdsink` and `shmsink` writers | an `Unsupported` error naming `container` | the whole file is `cfg(unix)` and behind a default off feature |
| `tauri-app/settings.rs`, the token file | `chmod 0600` | no-op | a shared Windows machine is a shared machine |

Shell plugins (`gmx plugin new --lang shell`) are refused at launch on Windows
because they need `sh`. The Rust, Python, Go and Node templates all work.

## Known upstream bugs

These are GStreamer issues, not GodwinMix ones, and the code works around each
of them. They are listed so that when one is fixed upstream the workaround can
be taken out rather than carried forever.

| Bug | Effect | What the code does |
|---|---|---|
| [`wasapi2sink` stutter #2870](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/2870) and [#3339](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/3339) | audio out stutters on some Windows devices | falls back to `directsoundsink`, which is older and steadier |
| [`mfvideosrc` startup #2748](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/issues/2748) | the camera fails to start on some Windows devices | falls back to `ksvideosrc` |
| `rtmp2src` delivers nothing against some servers, and the fallback waits six seconds | a source that looks connected and is silent | the probe on first connect runs both clients in parallel for two seconds and keeps the one that delivers |
| GPU compositors at rank none upstream, with open bugs | a compositor that is selected and then misbehaves | the software compositor is the default; a GPU entry is promoted only after `gmx codec test` and a sixty minute add and remove soak |
| D3D12 rebuild leak | memory grows over a long show on Windows | one shared D3D11 or GL device across pipelines, and D3D12 is not promoted |
| No codec enabled CEF on macOS and Windows | web page sources cannot play H.264 video | the project's CI builds and publishes one; superimposing a page over a video source stays the cheap path |
| NDI licensing | cannot be a build dependency | the runtime is `dlopen`'d, never linked, and the plugin carries the attribution |

## How this is kept honest

Three things, all of which run without anybody remembering to run them.

`.github/workflows/build.yml` builds and tests on all three platforms on every
push. A `cfg(windows)` arm that stops compiling fails there, in the job named
for the platform.

`.github/workflows/platforms.yml` goes further on every push: it starts the
core headless with a `test://` source, runs the smoke test end to end
(`dev/smoke.sh`, or `dev/smoke.ps1` which is a step for step port of it),
builds the desktop bundle with the trimmed runtime inside, runs
`--headless-check` to prove the bundled plugins are the ones that load, and
measures the installer against the budget. The Windows installer budget is 150
MB and going over it fails the job.

And the refusals are tested as refusals. `handshake.rs` and `transport.rs`
both carry `#[cfg(not(unix))]` tests that assert the Windows error message
names `container`, so a refusal that stops naming the way forward fails CI on
the Windows runner rather than reaching somebody's first launch.

## See also

* [Install on Windows](../how-to/install-on-windows.md),
  [macOS](../how-to/install-on-macos.md),
  [Linux](../how-to/install-on-linux.md)
* [The desktop app](../how-to/desktop-app.md)
* [Why plugins are processes](why-plugins-are-processes.md)
* [Why the programme never stops](why-the-programme-never-stops.md)
