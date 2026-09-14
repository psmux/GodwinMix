# Add a built in source kind

A built in kind is tier 0: Rust compiled into the core, registered behind the
same trait a third party plugin uses. It gets no privileges the contract does
not give everyone.

Read `docs/explanation/how-a-source-works.md` first if you have not. This is the
mechanical part.

The worked examples here are two things that were added this way and are small
enough to read in one sitting: `src/plugin/outputs/srt.rs`, which is an entire
new output in one file, and `src/plugin/filters/chroma.rs`, which is an entire
new filter in one file.

## 1. Write the file

`src/plugin/kinds/<name>.rs`. Four things go in it.

**A manifest**, as a `const`. It is the Rust mirror of one `[[provides]]` entry
in a plugin's `gmx-plugin.toml`:

```rust
pub const MANIFEST: Manifest = Manifest {
    plugin: "ndi",
    id: "source",                      // the full id is "ndi/source"
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "An NDI sender on the local network",
    uri_schemes: &["ndi://"],
    rank: 200,                         // highest rank wins a bare URI
    media: MediaDecl {
        video: StreamMode::Raw,        // Raw, Container or None
        audio: StreamMode::Raw,
        alpha: false,
        thumb: true,
    },
    capabilities: CapabilitySet::new()
        .with(Capability::RestartInPlace)
        .with(Capability::Health),
    latency_ms: 80,
    tier: Tier::Core,
};
```

Declare only capabilities you actually honour. `restart-in-place` means the
supervisor will NULL your pipeline and start it again rather than building the
source from nothing; if that does not work for you, leave it off and you get the
rebuild with the freeze frame instead.

**A claim function and a provide**, which is how a bare URI finds you:

```rust
pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: new };

fn claims(uri: &str) -> Option<u16> {
    uri.trim().to_lowercase().starts_with("ndi://").then_some(MANIFEST.rank)
}
```

Return `None` for a URI you do not want. A kind that should never be chosen from
a URI (`layered/source` is one) returns `None` always and is reachable only by
an explicit `type`.

**A struct and the trait.** The interesting method is `start`. Build only what
sits above the canvas capsfilters and let `assemble` do the rest:

```rust
fn start(&mut self, canvas: &CanvasCaps, thumb: bool) -> Result<MediaEnds> {
    self.ctx.canvas = canvas.clone();
    let src = make("ndisrc", &format!("{}-src-ndi", self.ctx.id))?;
    src.set_property("url-address", &self.address);
    assemble(
        &self.ctx,
        thumb,
        Ingest::default().with([src.clone()]).livesync(true),
        |w: &Wiring| {
            w.route(&src, w.norm.video_entry(), w.norm.audio_entry());
            Ok(KindParts::default())
        },
    )
}
```

`w.route` handles dynamic pads. If your element has static pads, link them
yourself inside the closure and set `w.has_video` / `w.has_audio` so the status
says what the source actually carries. `src/plugin/kinds/testsrc.rs` is the
shortest example of the static pad case; `src/plugin/kinds/rtmp.rs` is the
longest example of the dynamic one.

`livesync(true)` for anything that drifts against our clock. `false` for a file
and for anything that paces its own output, such as a process on a pipe.

**A `validate` function** for `params`. A bad param names the field and the
values it accepts. Copy the shape from `chroma.rs`:

```rust
pub fn validate(params: &Params) -> Result<()> {
    for (key, value) in params {
        match key.as_str() {
            "name" => anyhow::ensure!(value.is_str(), "ndi/source params.name must be a string"),
            _ => {}
        }
    }
    Ok(())
}
```

## 2. Register it

Three lines, all in files that already exist.

`src/plugin/kinds/mod.rs`:

```rust
pub mod ndi;
```

`src/plugin/source.rs`, in `REGISTRY`:

```rust
kinds::ndi::PROVIDE,
```

`src/config.rs`, in `SourceConfig::validate_params`:

```rust
"ndi" => crate::plugin::kinds::ndi::validate(&params),
```

That is the whole registration. There is no other list, no `SourceKind` variant
to add, no arm in a `match` inside `build_kind`, and no touch point in
`mixer.rs`.

## 3. Run the harness

The conformance checks are the same ones a third party plugin runs:

```
cargo run -- --test-core
```

That checks every built in kind that needs no network. For a kind that does need
one, write a test that calls the harness directly. The file source's test is the
pattern to copy (`src/plugin/harness.rs`, `a_file_source_passes_the_same_checks`):
it writes a clip with GStreamer, runs the checks against it, and removes the
clip.

```rust
#[test]
fn my_kind_is_conformant() {
    let _ = gst::init();
    let cfg = SourceConfig::bare("harness-ndi", "ndi://CAM 1");
    let report = crate::plugin::harness::check_source(&cfg, false).expect("the harness runs");
    for line in report.lines() {
        println!("{line}");
    }
    report.into_result().expect("ndi/source is conformant");
}
```

The checks, and what failing each one means:

| Check | What it means when it fails |
|---|---|
| `playing` | the pipeline did not reach PLAYING in ten seconds |
| `video caps` | what arrives at the proxy sink is not the canvas contract; almost always a missing convert or scale above the normaliser |
| `audio caps` | the same on the audio side, or the kind declared audio it does not deliver |
| `video buffers` | fewer than ninety percent of the expected frames in three seconds, or a PTS went backwards |
| `audio buffers` | the same for audio |
| `stop` | the pipeline did not go to NULL, or the kind held on to something |

Then run the descriptor and directory counting tests in `src/input.rs` if your
kind starts a process or writes a file:

```
cargo test --lib leaves_no_descriptors
```

## Adding an output instead

Smaller. `src/plugin/outputs/<name>.rs` with a manifest, `claims`, and an
`Output` impl whose `build` makes a muxer and a sink and links the two queues
the core hands you. Register it in `REGISTRY` in `src/plugin/output.rs`.
Everything that rides out a network outage (the feed queues on the programme
side, the proxy pair, the reconnect backoff, the overflow watchdog, the keyframe
request on reconnect) is already done for you and you must not repeat it.

The one thing worth care is `connected()`. Answer it from the sink's own
statistics, not from data flowing: a sink that connects in the background
accepts buffers straight away and says nothing about whether anyone answered.
`srt.rs` shows the defensive version, trying several field names and falling
back to the element's state on a build whose sink reports nothing.

## Adding a filter instead

`src/plugin/filters/<name>.rs` with a manifest and a `Filter` impl whose `build`
returns a `gst::Bin` with `sink` and `src` ghost pads. Register it in
`plugin::filter::make`.

Two rules. A filter must hand back exactly the caps it was given: everything
downstream of a source's capsfilter is interchangeable and a filter that changed
that would break the take, which is why `chroma.rs` ends its bin with a
capsfilter on the canvas caps. And it must not hold buffers: a queue inside a
filter is latency the programme did not agree to.
