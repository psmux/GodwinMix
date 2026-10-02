# Frame bus

The crate `godwinmix-framebus`. One owner decodes a device or a stream once
and publishes each decoded frame into shared memory; any number of readers, in
the same process or in others, read the frames there without a copy. Why it is
built this way, and what it costs, is in
[the explanation](../explanation/frame-bus.md).

The mixer uses it for every source whose plugin declares
[`share`](plugin-manifest.md#share): the camera, screen capture and channel
streams (`ingest/rtmp` reading a channel). The first source to ask opens the
device; every source, that one included, reads the bus. The mixer's side is
described at the end of this page.

## Names

| Name | For |
|---|---|
| `camera:<id>` | a device this machine opens: a camera, a capture card |
| `channel:<app>/<stream>` | a stream arriving on a channel |
| `<either>#audio` | the sound of that device or stream, published beside its pictures |

Each part is 1 to 64 letters, digits, `-` or `_`. Anything else is refused with
`bad-name` and a sentence showing the forms.

## The registry

A registry is a directory with one Unix socket per published name in it:
`camera=<id>.sock`, `channel=<app>+<stream>.sock`, and the same with `#audio`
before `.sock` for sound. The station makes it and passes it to each show it
starts as `GODWINMIX_BUS_DIR`. Without that variable the crate uses
`$XDG_RUNTIME_DIR/godwinmix/bus` on Linux, or `godwinmix-<uid>/bus` in the
temporary directory, and the mixer uses `bus` under its home, `~/.godwinmix/bus`
(or `$GODWINMIX_HOME/bus`), so that two mixers started by hand on one machine
find each other until shows exist. The directory is made mode 0700.

Beside each socket there is a lock file, `camera=<id>.lock`: whoever holds an
exclusive `flock` on it is the one that opens the device (`Claim`). The kernel
lets go of it when the holder's process dies, however it dies. The files stay
after the owner has gone; they are empty and cost nothing.

A socket path longer than 103 bytes cannot be bound on macOS (107 on Linux),
and a long `GODWINMIX_HOME` makes one easily. When the path in the registry
would be too long, the socket is bound and found through a short symbolic link
to the registry instead: `/tmp/gmx-<uid>/<16 hex digits>`, or the same under
`$XDG_RUNTIME_DIR` on Linux, where the digits are a hash of the registry's
path, so every process finds the same link without being told. The socket
itself still lands in the registry, beside its lock file, and the listing and
the claims are unchanged. `gmx-<uid>` is made mode 0700, and one that belongs
to somebody else or that others can write to is refused rather than used. A
name too long even for the short address is refused, naming
`GODWINMIX_BUS_DIR` as the way to a shorter directory. Windows has no frame bus
transport yet, so none of this applies there.

An owner that finds a socket for its name connects to it first. If something
answers, the name is taken (`name-taken`). If nothing does, the socket was left
by an owner that died, and it is replaced.

## Rust surface

```rust
use godwinmix_framebus::{BusName, Format, Layout, Publisher, PublisherOptions, Registry, Subscriber};

let reg = Registry::from_env()?;
let name: BusName = "camera:cam-wide".parse()?;

// Owner.
let layout = Layout::new(Format::Nv12, 1920, 1080)?.with_fps(30, 1);
let mut owner = Publisher::create(&reg, &name, layout, PublisherOptions::default())?;
owner.write(Some(pts_ns), None, |slot| decode_into(slot));   // or write_planes, or push_buffer
let stats = owner.stats();                                     // published, dropped, per reader

// Reader, here or in another process.
let mut reader = Subscriber::connect(&reg, &name)?;
if let Some(frame) = reader.next(Duration::from_millis(100))? {
    let y = frame.plane(0);                                    // borrowed from shared memory
    let missed = frame.skipped();
}                                                              // dropping the frame gives the slot back
```

### `Claim`

| Call | Does |
|---|---|
| `Claim::try_take(registry, name)` | the right to open what `name` names, or `None` when a live process (or another `Claim` in this one) holds it. Dropping it gives it back |
| `Claim::is_held(registry, name)` | whether somebody holds it now |

### `Publisher`

| Call | Does |
|---|---|
| `create(registry, name, layout, options)` | binds the name's socket and starts the bus thread |
| `write(pts, duration, fill)` | claims a slot, runs `fill` on its bytes, publishes it. `false` if every slot was leased and the frame was dropped |
| `write_len(pts, duration, len, fill)` | the same, filling only the first `len` bytes, for a chunk of sound |
| `write_planes(pts, planes)` | the same, copying planes given as `(bytes, stride)` row by row into the slot layout |
| `push_buffer(buffer, video_info)` | the same for a GStreamer buffer; changes the layout first if the caps changed (`gst` feature) |
| `set_layout(layout)` | a new format: readers are handed a new region, and frames they hold from the old one stay valid |
| `stats()` | `published`, `dropped`, `slots`, and per reader `pid`, `delivered`, `skipped`, `holding` |

Dropping the `Publisher` removes its socket. Readers see the owner go and wait
for the next one.

### `PublisherOptions`

| Field | Default | Meaning |
|---|---|---|
| `max_readers` | 8 | readers attached at once, at most 32. One more is refused with a sentence saying so |
| `leases_per_reader` | 3 | frames one reader may hold at once. A reader holding that many waits for one of its own to drop; the owner never waits |
| `checksum` | false | write an FNV-1a checksum of each frame into its slot, for `Frame::verify` |

The region has `max_readers * leases_per_reader + 2` slots, at most 64. Slots
are allocated lazily by the operating system and the owner reuses the lowest
free one, so a region with room for 26 frames touches four or five while its
readers keep up.

### `Subscriber` and `Frame`

| Call | Does |
|---|---|
| `Subscriber::connect(registry, name)` | attaches; `not-found` when nothing publishes the name |
| `next(timeout)` | the newest frame this reader has not seen, or `None` when none came in time. Never an older one |
| `is_connected()`, `reconnects()`, `layout()` | the reader's state |
| `Frame::data()`, `plane(i)`, `layout()` | the frame's bytes, in place |
| `Frame::seq()`, `skipped()` | the owner's frame number, and how many frames this reader missed before this one |
| `Frame::pts()`, `duration()` | as the owner gave them |
| `Frame::captured_ns()`, `published_ns()` | when the owner was handed the frame and when it became readable, on `monotonic_ns()`, which every process on the machine shares |
| `Frame::verify()` | `Some(true)` if the bytes match the owner's checksum, `None` if it writes none |

When the owner dies the reader keeps the frames it holds, `next` returns `None`,
and it attaches to the next owner of the name by itself, whatever format that
one has. `Frame::seq` starts again at 1.

### Errors

`Error::code()` is one of `bad-name`, `bad-layout`, `not-found`, `name-taken`,
`owner-gone`, `protocol`, `os`, `unsupported`. Each message says what happened
and what to do next.

## GStreamer elements

Call `godwinmix_framebus::gst::register()` once after `gst::init()`. The
elements are registered in the process, not installed as a plugin file, so
`gst-launch-1.0` does not know them.

```
... ! videoconvert ! video/x-raw,format=NV12 ! gmxbussink bus-name=camera:cam-wide
gmxbussrc bus-name=camera:cam-wide ! compositor ...
```

`gmxbussink`

| Property | Default | |
|---|---|---|
| `bus-name` | | required |
| `bus-dir` | empty | the registry directory; empty for `GODWINMIX_BUS_DIR` or the default |
| `max-readers` | 8 | as above |
| `leases` | 3 | frames per reader, as above |

It publishes on its first caps and changes format when the caps change.

`gmxbussrc` is a live source. `bus-name` and `bus-dir` as above. It starts
whether or not an owner exists yet and waits for one. Each buffer's memory is
the slot itself, with a `GstVideoMeta` for the plane offsets and strides; the
lease is returned when the buffer is freed. Each buffer also carries a
`GstReferenceTimestampMeta` with caps `timestamp/x-gmx-monotonic` holding the
time the owner was handed the frame, and its offset is `Frame::seq`. Caps come
from the owner and change when the owner's do.

| Property | Default | |
|---|---|---|
| `timestamps` | `arrival` | `arrival` stamps each buffer when it arrives, on this pipeline's clock. `owner` keeps the owner's timestamps and durations, for a reader that puts them on its own timeline itself, as the mixer does to keep a stream's sound and pictures together |

Both elements carry sound under a name ending `#audio`: interleaved `F32LE` or
`S16LE`, any rate and 1 to 64 channels. The sink cuts a buffer into chunks of
at most 100 ms, each stamped with where it starts.

## Formats

NV12, I420, P010_10LE, YUY2, UYVY, BGRA, RGBA and BGRx, at any size from 1x1 to
16384x16384. Rows are padded to 64 bytes. Anything else is refused with
`bad-layout`, which names `videoconvert` to NV12 as the fix.

Sound is `F32LE` or `S16LE`, interleaved (`Layout::audio(format, rate,
channels)`). A slot holds up to 100 ms and says how much of it a chunk filled.
A sound region is used differently from a picture region: the owner overwrites
the oldest free chunk rather than the lowest numbered one, and a reader takes
the chunk after the one it had rather than the newest, so it hears every chunk
in order. A reader that falls more than 8 chunks behind skips to the newest, and
`skipped` counts what it missed.

## Platforms

| | Region | Handed over as | Notices a dead reader by |
|---|---|---|---|
| Linux | `memfd_create` | descriptor over the socket (`SCM_RIGHTS`) | its socket closing |
| macOS | `shm_open`, unlinked at once | the same | the same |
| Windows | not yet | | |

On Windows the crate builds, `CROSS_PROCESS` is false and `available()` returns
`unsupported`; a show there decodes its own sources, as every consumer does
today.

## Limits

| | |
|---|---|
| readers per name | 32 |
| slots per region | 64 |
| frames one reader holds | `leases_per_reader`, at most 8 through `gmxbussink` |

## Benchmark

`dev/framebus-bench.sh` builds and runs the matrix. Arguments go to
`framebus-bench matrix`:

| Argument | Default | |
|---|---|---|
| `--seconds` | 10 | the measured window |
| `--readers` | `1,4,8` | reader counts |
| `--decoder` | `avdec_h264` | any H.264 decoder GStreamer has, `vtdec_hw` for VideoToolbox |
| `--format` | `NV12` | what the owner publishes |
| `--repeat` | 1 | runs per scenario; the median by total CPU is kept |
| `--out` | | also write the table to this file |

The clip is made once, under `target/framebus-bench/`.

## In the mixer

`crates/godwinmix-core/src/plugin/host/shared/` is the mixer's side. A source
whose provide declares `share` is a `SharedSource`:

* It always reads the bus: `gmxbussrc` in front of the normaliser every source
  has. A picture alone is stamped on arrival. Pictures and sound together keep
  the owner's timestamps and are moved onto the mixer's timeline by one shift
  shared by both, so they stay in step.
* A thread of its own asks for the name's claim every 50 ms. The source that
  gets it starts the plugin, feeds what the plugin sends through a normaliser
  whose `videorate` only drops (so it holds no frame back), and publishes it
  with `gmxbussink` (16 readers, 8 frames each).
* When the owner's plugin fails, or sends nothing for 3 s, it gives the claim
  back and waits a second before asking again, so another source can try.
* When the owner goes, its claim goes with it and the first reader to ask takes
  it, starts the plugin, and publishes under the same name. Every reader's
  `gmxbussrc` finds the new owner by itself.

No protocol method answers with these numbers yet. The source answers
`share` through the `Source` trait's `call`, and the mixer's log has the same:
`this source opened the device and shares it` with how long the plugin took to
start, `the picture came back after a gap` with the gap, and `stopping a shared
source` with the whole report:

| Field | |
|---|---|
| `bus`, `dir` | the name and the registry |
| `owner`, `plugin_pid`, `takeovers`, `last_start_ms` | whether this source runs the plugin, which process that is, how often it became the owner, and how long the plugin took to start the last time |
| `publish_ms` | the owner only: from a frame reaching the mixer from the plugin to its publish, p50, p99 and max |
| `hop_ms` | from the owner's publish to this source holding the frame |
| `to_programme_ms` | from the owner's publish to the frame leaving this source for the programme |
| `frames`, `longest_gap_ms`, `gaps`, `last_gap_ms` | frames that left, and the gaps between them; `gaps` counts those over 250 ms |

`GODWINMIX_FRAMEBUS=off` in the mixer's environment turns sharing off: every
source opens its own device, as before.
