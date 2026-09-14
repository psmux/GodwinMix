# Write a source plugin in Rust

A source is a process that produces pictures, sound, or both, at the canvas the
core is running. `godwinmix-sdk` gives you the protocol loop, the pacing, the
frame pool and the media transports, so what you write is the part that draws.

The worked example on this page is
[`crates/godwinmix-sdk/examples/colour-bars.rs`](../../crates/godwinmix-sdk/examples/colour-bars.rs),
a complete source in 149 lines. Every piece of code quoted here is from
that file or compiled against the crate.

## Add the dependency

The crate is not published yet, so depend on it by path:

```toml
[dependencies]
godwinmix-sdk = { path = "../godwinmix/crates/godwinmix-sdk" }
serde_json = "1"
```

It pulls in serde, serde_json and toml, and nothing native. `serde_json::Value`
turns up in the trait signatures, so you want it in your own `Cargo.toml` too.

One `use` brings in everything a source needs:

```rust
use godwinmix_sdk::prelude::*;
use serde_json::Value;
```

The example adds two more from the standard library, because it keeps its one
setting in an atomic the media thread reads:

```rust
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
```

## The manifest you need

`gmx-plugin.toml` sits at the root of the plugin. This is the example's, and it
is close to the minimum for a compiled source:

```toml
[plugin]
name = "colour-bars"
version = "0.1.0"
api = 1
description = "Eight colour bars at the canvas caps, with an optional sideways drift. A test picture that costs almost nothing to draw."
license = "Apache-2.0"
authors = ["GodwinMix"]
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64", "windows-x86_64"]
placements = ["sidecar", "node"]
process = "per-instance"

[run]
bin = { "linux-x86_64" = "bin/colour-bars", "linux-aarch64" = "bin/colour-bars", "macos-aarch64" = "bin/colour-bars", "macos-x86_64" = "bin/colour-bars", "windows-x86_64" = "bin/colour-bars.exe" }

[[provides]]
kind = "source"
id = "source"
media = { video = "raw", audio = "none", alpha = false, thumb = true }
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "colour-bars.settings.json"
```

A source provide must declare `media`, `transports` and `settings`; the
validator refuses it otherwise. Every key, and every rule, is in
[the manifest reference](../reference/plugin-manifest.md).

`runtime::run` reads this file and sends it in the handshake, so the core never
parses it twice:

```rust
fn main() {
    let env = PluginEnv::from_env();
    let manifest_path = env.root.join("gmx-plugin.toml");
    let manifest = match Manifest::load(&manifest_path) {
        Ok(m) => m,
        Err(_) => Manifest::parse(include_str!("colour-bars.toml")).expect("the built in manifest"),
    };
    if let Err(e) = runtime::run(&manifest, SourceHandler(ColourBars::new())) {
        eprintln!("colour bars stopped: {e}");
        std::process::exit(1);
    }
}
```

`PluginEnv::from_env` reads the `GMX_*` variables the core sets, and
`env.root` is `GMX_PLUGIN_ROOT`. The example falls back to a compiled in
manifest so it runs from a checkout; a real plugin should let the error out.

## Implement `Source`

Six methods matter. Four have no default and you must write them; `health` has
one you should replace; `call` catches anything the core adds later.

| Method | Signature | You must write it |
|---|---|---|
| `initialize` | `fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError>` | yes |
| `start` | `fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError>` | yes |
| `stop` | `fn stop(&mut self) -> Result<(), RpcError>` | yes |
| `configure` | `fn configure(&mut self, params: Value) -> Result<Configure, RpcError>` | yes |
| `health` | `fn health(&mut self) -> Health` | defaults to `Health::ok()` |
| `call` | `fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError>` | defaults to method not found |

`seek`, `position`, `audio_set` and `keyframe` are there too, each defaulting to
an error that names the capability you have to declare before the core will call
it. Declare `seek` in the manifest and implement `seek` and `position`; declare
`audio-layers` and implement `audio_set`.

`initialize` is where the handshake result arrives. Keep what you need from it:

```rust
fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
    self.canvas = ready.canvas;
    self.drift.store(drift_from(&ready.params), Ordering::Relaxed);
    reporter.info(format!(
        "colour bars at {}x{}@{} for instance '{}'",
        ready.canvas.width, ready.canvas.height, ready.canvas.fps, ready.instance
    ));
    self.reporter = Some(reporter);
    // Bars are drawn on demand, so nothing is buffered and nothing is late.
    Ok(InitializeResult { latency_ms: Some(0) })
}
```

`ready.params` is your settings object, already validated against the schema the
manifest points at, so there is nothing to check. `Reporter` is cheap to clone
and safe to hold in the media thread: `info`, `warn`, `error`, `set_health`,
`health_changed`, `media_report` and `event` are each one line on stderr.

The `latency_ms` you return is what the core answers the latency query with,
when the provide declares `latency-report`. Return what you actually add.

## Open the media writer and drive it

`start` carries the transport the core chose and its address. `media::open`
turns that pair into a writer:

```rust
fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
    let canvas = params.canvas;
    self.canvas = canvas;
    let writer = media::open(
        params.transport,
        &params.media,
        canvas,
        media::Streams::video_only(media::VideoFormat::I420),
    )
    .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))?;
    let drift = Arc::clone(&self.drift);
    self.media = Some(VideoLoop::spawn(
        canvas,
        writer,
        self.reporter.clone(),
        move |frame, pts| {
            let px_per_sec = drift.load(Ordering::Relaxed) as f64 / 1000.0;
            let seconds = pts as f64 / 1_000_000_000.0;
            draw_bars(frame, canvas, (px_per_sec * seconds) as i64);
        },
    ));
    Ok(StartResult { latency_ms: Some(0) })
}
```

`Streams` says what you intend to send before the transport opens:
`video_only(format)`, `video_and_audio(format)` or `audio_only()`. The format is
`VideoFormat::I420`, or `VideoFormat::Ayuv` when the provide declares
`alpha = true`.

`VideoLoop::spawn` owns a thread and calls your closure once per frame:

```rust
pub fn spawn<F>(canvas: Canvas, writer: Box<dyn MediaWriter>, reporter: Option<Reporter>, draw: F) -> VideoLoop
where
    F: FnMut(&mut [u8], u64) + Send + 'static
```

For sound as well, `VideoLoop::spawn_with_audio` takes a second closure and
calls it for each 10 ms buffer due before the next video frame, so the two stay
level without a burst after a slow frame.

`stop` is then one line, because dropping the loop stops the thread and finishes
the writer:

```rust
fn stop(&mut self) -> Result<(), RpcError> {
    self.media = None;
    Ok(())
}
```

`start` may follow `stop`. Do not hold the writer open across it.

## What `draw` must produce

The media contract does not negotiate:

```
video   I420, BT.709, canvas width x height, canvas fps, one frame per buffer
        AYUV when the provide declares alpha = true
audio   F32LE interleaved, 48 kHz, 2 channels, 10 ms per buffer
time    PTS in nanoseconds on the plugin's own monotonic clock, starting near 0
```

The buffer your closure is handed is already the right length. An I420 frame is
three planes end to end:

| Plane | Size | What it carries |
|---|---|---|
| Y | `width * height` | brightness, 16 is black and 235 is white |
| U | `ceil(width/2) * ceil(height/2)` | blue difference, 128 is neutral |
| V | `ceil(width/2) * ceil(height/2)` | red difference, 128 is neutral |

`Canvas::i420_frame_bytes()` is that sum, and `Canvas::ayuv_frame_bytes()` is
`width * height * 4`. At 1920x1080 an I420 frame is 3,110,400 bytes; at 1280x720
it is 1,382,400. Chroma planes round up, so an odd width loses no column.

Splitting the planes is the first thing `draw_bars` does:

```rust
let width = canvas.width as usize;
let height = canvas.height as usize;
let cw = width.div_ceil(2);
let ch = height.div_ceil(2);
let (y_plane, chroma) = frame.split_at_mut(width * height);
let (u_plane, v_plane) = chroma.split_at_mut(cw * ch);
```

The second thing it does is draw one row of each plane and copy it down, because
the bars do not change with the row:

```rust
for row in 1..height {
    y_plane.copy_within(0..width, row * width);
}
```

That matters more than it looks. At 1080p30 your closure has 33 milliseconds,
and it is called 30 times a second forever.

## `configure`

The core sends the full validated settings object, never a diff, and it can
arrive at any time including before `start`. Answer one of two shapes:

```rust
Ok(Configure::applied())
Ok(Configure::restart_required("the frame pool is sized at start"))
```

`Configure::applied()` means the new params are live. `restart_required(reason)`
means they are not, and the core turns that into error -32012 for the caller
with your reason in it, so the operator knows to call `plugin.reload` and why.
Never panic here, and never return an error for a value the schema already
allowed.

The example keeps its one setting in an atomic that the media thread reads each
frame, so a change lands on the next frame and nothing stops:

```rust
fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
    self.drift.store(drift_from(&params), Ordering::Relaxed);
    Ok(Configure::applied())
}
```

That is the shape to copy where you can. Where a setting really does need the
loop rebuilt, `stop()` then open it again and still answer `applied`; the core
covers the gap with a freeze frame. Keep `restart_required` for the settings
that need the process itself restarted.

## `health`, and why it must be fast

```rust
fn health(&mut self) -> Health {
    match self.media.as_ref() {
        Some(loop_) if !loop_.is_running() => Health::failing("the media loop stopped"),
        _ => Health::ok(),
    }
}
```

Three states: `Health::ok()`, `Health::degraded(detail)` and
`Health::failing(detail)`. The core asks while other calls are in flight, and a
plugin that cannot answer while a slow `start` is pending looks stalled to the
supervisor.

The SDK keeps that promise for you. The reader thread answers `health` from a
cached value without touching your plugin, and the worker thread refreshes that
value after every call it runs. So `health` is never blocked, and the only thing
you must not do is spend milliseconds inside the method itself. Do not open a
socket in there, and do not take a lock the media thread holds.

To push a change rather than wait to be asked, call
`reporter.health_changed(Health::degraded("the camera dropped to 15 fps"))` from
wherever you noticed. It updates the cached answer and sends a notification in
the same call.

## The `gst` feature

The crate has no native dependency by default and writes Matroska itself. Two
reasons to turn GStreamer on:

```toml
godwinmix-sdk = { path = "...", features = ["gst"] }
```

* You want the `unixfd` or `shm` transport. `unixfdsink` and `shmsink` cannot be
  written from pure Rust, and both are Unix only.
* Your plugin already links GStreamer, and muxing with `matroskamux` saves
  carrying the SDK's writer as well.

`unixfd` is zero copy, memfd or DMABUF backed, and one sink can feed several
readers. `shm` is one copy per frame. `container` is a Matroska stream on
stdout, costs one copy into the pipe, and works everywhere including Windows.
Declare what you can do and let the core choose:

```toml
transports = ["unixfd", "container"]
```

Ask for `unixfd` in a build without the feature and `media::open` fails with a
message naming the way out, rather than a missing element deep in a pipeline:

> the core chose the 'unixfd' transport and this build of the SDK has no
> GStreamer: build it with --features gst. Declare transports = ["container"] in
> gmx-plugin.toml; a container on a pipe works everywhere and costs one copy per
> frame.

Start with `container`. It is the safe first choice, and 93 MB/s on a pipe at
1080p30 is usually cheaper than the day you spend on the other two.

## Test it offline

`runtime::run_on` takes any reader and writer, so a test drives the whole plugin
in process, with no core, no sockets and no clock. Record the conversation as a
transcript, one JSON object per line, `core` for a line the core sends and
`plugin` for a line your plugin must send:

```jsonl
# The handshake, then a start, a configure and a shutdown.
{"plugin": {"method": "initialize", "params": {"plugin": "grey", "api": 1}}}
{"core":   {"jsonrpc": "2.0", "id": 0, "result": {"core": "godwinmix", "version": "0.0.0", "api_level": 1, "api_compatible": 1, "canvas": {"width": 160, "height": 90, "fps": 30}, "transport": "container", "media": "", "instance": "test", "provide": "source", "params": {"level": 128}}}}
{"plugin": {"method": "initialized"}}
{"core":   {"jsonrpc": "2.0", "id": 1, "method": "start", "params": {"canvas": {"width": 160, "height": 90, "fps": 30}, "transport": "container", "media": ""}}}
{"plugin": {"id": 1, "result": {"latency_ms": 0}}}
{"core":   {"jsonrpc": "2.0", "id": 2, "method": "configure", "params": {"params": {"level": 200}}}}
{"plugin": {"id": 2, "result": {"applied": true}}}
{"core":   {"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {"reason": "offline test"}}}
{"plugin": {"id": 3, "result": {}}}
```

A `plugin` line matches as a subset: the real line must carry the keys you name
with the values you name, and may carry anything else. `"*"` matches whatever is
there, which is how a generated id or a timestamp is allowed to vary.

The test that replays it:

```rust
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use godwinmix_sdk::framing::{Reader, Writer};
use godwinmix_sdk::manifest::Manifest;
use godwinmix_sdk::plugin::SourceHandler;
use godwinmix_sdk::runtime;
use godwinmix_sdk::transcript::{self, Step};
use serde_json::Value;

/// Collects the control channel so the test can read it back.
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn the_recorded_transcript_replays() {
    let manifest = Manifest::parse(include_str!("../gmx-plugin.toml")).unwrap();
    let steps = transcript::steps(include_str!("transcript.jsonl")).unwrap();

    let stdin: String = steps
        .iter()
        .filter_map(|(_, step)| match step {
            Step::Core(v) => Some(v.to_string() + "\n"),
            _ => None,
        })
        .collect();

    let sink = Sink::default();
    runtime::run_on(
        &manifest,
        SourceHandler(Grey::new()),
        Reader::new(Cursor::new(stdin)),
        Writer::new(Box::new(sink.clone())),
    )
    .expect("the run ended cleanly");

    let said: Vec<Value> = String::from_utf8(sink.0.lock().unwrap().clone())
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    let mut at = 0;
    for (line, step) in &steps {
        let Step::Plugin(want) = step else { continue };
        match said[at..].iter().position(|got| transcript::matches(want, got)) {
            Some(offset) => at += offset + 1,
            None => panic!("line {line}: nothing matched {want}, the plugin said {said:#?}"),
        }
    }
}
```

`cargo test` runs it in milliseconds on any machine, with no GStreamer
installed. One surprise: in container mode the media really does go to the test
binary's stdout, so `cargo test` prints a few kilobytes of Matroska at you. That
is the plugin working, not a bug. Point the writer somewhere else if it bothers
you, by calling `media::ContainerWriter::new` yourself instead of
`media::open`.

`gmx plugin test --offline` will run a transcript against a built plugin the
same way. It does not exist yet.

To see the whole thing end to end, drive the example by hand the way the core
does:

```sh
cargo build -p godwinmix-sdk --example colour-bars
{
cat <<'EOF'
{"jsonrpc":"2.0","id":0,"result":{"core":"godwinmix","version":"0.0.0","api_level":1,"api_compatible":1,"canvas":{"width":320,"height":180,"fps":30},"transport":"container","media":"","instance":"bars","provide":"source","params":{"drift_px_per_sec":40}}}
{"jsonrpc":"2.0","id":1,"method":"start","params":{"canvas":{"width":320,"height":180,"fps":30},"transport":"container","media":""}}
EOF
sleep 2
echo '{"jsonrpc":"2.0","id":2,"method":"shutdown","params":{"reason":"by hand"}}'
sleep 1
} | target/debug/examples/colour-bars > bars.mkv 2> control.jsonl
cat control.jsonl
```

```
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"api":1,"plugin":"colour-bars","provides":[{"capabilities":["restart-in-place","health"],"id":"source","kind":"source","latency_ms":0,"media":{"alpha":false,"audio":"none","thumb":true,"video":"raw"},"settings":"colour-bars.settings.json","transports":["container"]}],"transports":["container"],"version":"0.1.0"}}
{"jsonrpc":"2.0","method":"log","params":{"level":"info","message":"colour bars at 320x180@30 for instance 'bars'"}}
{"jsonrpc":"2.0","method":"media.report","params":{"latency_ms":0}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":1,"result":{"latency_ms":0}}
{"jsonrpc":"2.0","method":"log","params":{"level":"debug","message":"media loop finished after 48 frames, 0 late"}}
{"jsonrpc":"2.0","id":2,"result":{}}
```

48 frames, none late, and the file `gst-discoverer-1.0` reads back as
`Uncompressed planar YUV 4:2:0`, 320 by 180, 30/1.

## What the SDK does for you that you would otherwise get wrong

**The pacing comes off the frame index, not off a sleep.** `Pacer` computes each
deadline as `frame * frame_duration`, so a frame that takes 40 ms at 30 fps
costs one late frame rather than shifting every frame after it. Add a sleep per
frame instead and your source drifts away from the canvas rate all evening.
`late_frames()` counts them, which is a good thing to put in `health`.

**PTS starts at zero on your own clock.** The core retimes onto programme
running time. A source that sends wall clock timestamps, or that restarts at a
different offset after `stop` and `start`, makes the aligner absorb a jump it
should never have seen. `Pacer::restart()` is called for you when the loop
starts.

**The frame pool.** At 1080p30 a fresh `Vec` per frame is 93 MB of allocation a
second. `FramePool` hands out three buffers and takes them back on drop, and
allocates rather than blocking if they are all out, because on a media thread
waiting costs a frame and a megabyte costs nothing.

**stdout is reserved for media.** In container mode stdout is the video stream,
and `println!` anywhere in your process corrupts it. The SDK takes the handle
and puts the control channel on stderr. Log through the `Reporter`, or with
`eprintln!`, which reaches the core's log at `info` as an unstructured line.

**The panic hook.** `runtime::run` installs one that writes a JSON crash report
into `GMX_PLUGIN_ROOT/crashes/` with the panic message, the backtrace and the
last fifty log lines this process produced, then logs the path so the core can
attach it to the alert an operator sees. A sidecar that dies silently is the
worst failure in a live show; this makes it the second worst.

## Also worth reading

* [The plugin manifest](../reference/plugin-manifest.md), every key and every
  validation rule.
* [The plugin protocol](../reference/plugin-protocol.md), the framing, the state
  machine and the error codes.
* [Your first plugin](../tutorials/your-first-plugin.md), the same path in
  Python, if you want the shape before the types.
