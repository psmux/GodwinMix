# ndi

NDI sources and outputs, with the runtime loaded at run time and never linked.

NDI is how a camera, a graphics machine and a mixer talk to each other on a
studio network without a capture card. A sender announces itself, a receiver
picks it by name, and nothing has an address to type.

| Provide | What it does |
|---|---|
| `ndi/source` | receive a sender from the network |
| `ndi/output` | announce this mixer's programme as a sender |
| `ndi/discover` | list every sender on the network, and the `list_senders` tool |

## Attribution and the licence

NDI® is a registered trademark of Vizrt NDI AB. GodwinMix is not a product of
Vizrt and is not affiliated with, endorsed by, or sponsored by it. More about
NDI, and NDI's own tools: <https://ndi.video/>.

The Windows installer carries the NDI runtime, `Processing.NDI.Lib.x64.dll`,
in this plugin's `bin` folder with `Processing.NDI.Lib.Licenses.txt` beside it.
The NDI SDK licence lets an application ship the runtime on three conditions,
and GodwinMix meets each of them:

* the DLL stays in the application's own folder, never a system one, so it
  cannot clash with another program's copy;
* the application's licence carries the NDI end user terms, which the Windows
  installer shows before it installs (`tauri-app/windows-licence.txt`);
* a link to ndi.video, with the trademark line, sits wherever NDI is picked.
  The settings schemas carry it as `x-gmx-notice`, and every form shows it.

`bin/gmx-ndi` sets `NDI_RUNTIME_DIR_V6` to its own folder when the DLL is
there, so `ndisrc` and `ndisink` load the copy that came with it.

On Linux and macOS, and in a build from a checkout, nothing is bundled. Install
the runtime from <https://ndi.video/for-developers/ndi-sdk/> (the runtime alone
is enough). The release job fetches the SDK on its Windows runner only.

The plugin `dlopen`s the runtime instead of linking against it. It builds on a
machine that has never seen NDI, runs on one, and refuses with the download
page rather than failing to load. DistroAV, formerly obs-ndi, documented the
packaging friction this design avoids.

## In four minutes

```sh
dev/harness/stage-plugins.sh
gmx plugin add ./plugins/ndi
gmx plugin describe ndi            # says whether the runtime is here
gmx source add cam1 --type ndi/source --params '{"name":"STUDIO (CAM 1)"}'
```

Run the binary by hand and it tells you what it can see, which is the quickest
way to check an installation:

```sh
plugins/ndi/bin/gmx-ndi
# the NDI runtime is at /usr/local/lib/libndi.so.6
#   STUDIO (CAM 1)  10.0.0.21:5961
```

## Where the runtime is looked for

In order: the bare library name, so the ordinary loader search
(`LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`, `PATH`) has its say first; then
`NDI_RUNTIME_DIR_V6`, `_V5` and `_V4`, which the NDI installers set; then the
usual directories for the platform.

| Platform | File |
|---|---|
| Linux | `libndi.so.6`, then `.so.5`, `.so.4`, `.so` |
| macOS | `libndi.dylib`, then `libndi.4.dylib` |
| Windows | `Processing.NDI.Lib.x64.dll` |

The library is opened, one known symbol is looked up, and it is closed again.
Nothing here calls into it: the media path is GStreamer's `ndisrc` and
`ndisink`, which do their own loading. This check exists so the plugin can
refuse with a sentence a reader can act on instead of "Failed loading NDI SDK".

## What it costs

```
source   ndisrc ──► ndisrcdemux ──┬─► queue ─► matroskamux ─► fdsink fd=1
                                  └─► queue ─┘

output   filesrc(the core's FIFO) ─► matroskademux ─┬─► decodebin ─► ndisinkcombiner ─► ndisink
                                                    └─► decodebin ─┘
```

NDI's own codec is SpeedHQ and only the runtime can decode it, so `ndisrc` hands
over raw frames and raw frames are what cross to the core: one memcpy per frame,
no encode and no decode, on a pipe carrying 41 MB/s at 720p30 and 93 MB/s at
1080p30. Those are the numbers the media contract quotes for this transport.

An `unixfd` transport would remove even that copy and is the obvious next step.
It is not here because it cannot be tested on a machine with no NDI runtime, and
shipping an untested transport is worse than shipping one copy per frame.

`bandwidth: "low"` is a real saving and the right answer for a source that is
only ever in a corner of a multiview.

The output pays a decode: the core hands it the programme already encoded and
`ndisink` wants raw. That is the protocol's price, not this plugin's. On a
machine already tight for CPU, an NDI output is the first thing to question.

## What the core cannot do yet

`ndi/source` works: sources go through the plugin loader. `ndi/output` and
`ndi/discover` do not, because:

* the output registry in `crates/godwinmix-core/src/plugin/output.rs` is a
  static list of the built in outputs and does not consult the loader, the way
  `plugin/source.rs` already does;
* `crates/godwinmix-core/src/plugin/loader.rs` interns only provides whose
  `kind` is `"source"`, so a `device` provide registers nothing, and nothing
  calls `discover`;
* `tool.call` is not a registered method, so `list_senders` cannot be reached
  through MCP or REST yet.

Both provides are complete against the contract and are exercised by
`gmx plugin test plugins/ndi --offline --provide discover`, which runs with no
runtime and no core.

## Settings

### `ndi/source`

| Key | Default | What it does |
|---|---|---|
| `name` | empty | the NDI name as announced, `STUDIO (CAM 1)` |
| `address` | empty | `host:port`, for a sender mDNS cannot reach |
| `bandwidth` | `high` | `low` is a much smaller picture at a fraction of the bandwidth; `audio-only` takes the sound alone |
| `timestamp_mode` | `receive-time-vs-timecode` | which clock frames are stamped with. Leave it alone unless lip sync is wrong across several NDI sources |

### `ndi/output`

| Key | Default | What it does |
|---|---|---|
| `name` | `GodwinMix` | what everybody else on the network sees this mixer called |

## When a sender cannot be found

1. Both machines must be on the same subnet. NDI finds senders with mDNS and
   mDNS does not cross a router; give `address` instead.
2. `list_senders` with a `timeout_ms` under 500 usually finds nothing even when
   senders are there. The announcements take about a second.
3. Some networks filter multicast. Managed switches and guest wifi both do.

## Testing it

```sh
cargo test -p gmx-ndi                        # says so and skips where NDI is absent
gmx plugin test plugins/ndi --offline --provide discover
gmx plugin test plugins/ndi                  # needs the runtime and a sender
```

Every test that needs the runtime prints a line saying it was skipped rather
than passing quietly, because a check that cannot run is not a check that
passed.

## Where the rules come from

`docs/reference/plugin-manifest.md`, `docs/reference/plugin-protocol.md` and
`docs/reference/plugin-lifecycle.md`. The how to page is
`docs/how-to/use-ndi.md`.
