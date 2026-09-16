# Where GodwinMix stands

Written 2026-09-15 at the v0.2.0 tag. Read this first if you are picking the
work up fresh. The product requirements live outside this repository, in the
private `~/workspace/modulargodwinmix` (remote `psmux/modulargodwinmix`), and
must never be copied into this one.

## Built and merged

Everything in the plan's phases 0 to 6 except the items listed below. The
protocol and generated clients, the plugin host with twelve first party
plugins, the SDK and templates, scenes with the composer, safety and the agent
surface, presets and themes, hooks, session replay and evals, transitions,
nodes, the WASM tier, OGraf graphics, the desktop app with a bundled
GStreamer, the Docker image and the docs beside each of them.

Verified on an Apple M4 Pro: `cargo test --workspace`, `cargo clippy
--workspace --all-targets -- -D warnings`, `dev/smoke.sh`, `dev/ui-tests.sh`,
the three client suites and `python3 clients/gen/generate.py --check`.

## Known open items, in the order they matter

1. **The mixer command loop can wedge.** A core left running for hours answers
   `/api/v1/core/info` (which does not touch the mixer) and hangs forever on
   `/api/status` and `/metrics` (which do). The web UI then loads and draws
   nothing. Seen after a long idle run with an output reconnecting to an RTMP
   server that was not there. The suspect is the watchdog rebuild holding the
   loop until the 256 command queue fills: `crates/godwinmix-core/src/mixer.rs`
   around the supervisor tick, written up as Codex finding 12 with the
   measurement that a shorter `BLOCK_TIMEOUT` made removals seventeen times
   worse (`crates/godwinmix-core/src/mixer/slots.rs:78`). Reproduce by running
   a core overnight with an output pointing at a closed port. This is the one
   defect that stops somebody using the product.
2. **CI ran for the first time on 2026-09-15** and found what had only ever
   been built on Apple silicon: a test helper not gated to Unix (Windows did
   not compile), `c_char` hardcoded as `i8` (aarch64 Linux did not compile),
   a race in the WASM worker that marked an instance spent after answering,
   and wall clock timing tests that a shared runner cannot hold. All four are
   fixed on 2026-09-16; the hosted jobs set `GODWINMIX_TIMING_SLACK=3` and
   nightly keeps the strict 34 ms. The second Windows run found thirteen
   more, all fixed the same day: a stderr reader joined before its child
   was killed (a hang), Windows paths written into pipeline descriptions,
   `unixfdsink` asked for on a platform without it, and a CRLF checkout
   inflating the UI byte budget. The Linux runners then exposed a real
   defect: a thumbnail branch was linked to a live tee before it was brought
   up, so a buffer could meet a flushing pad and pause the source's loop for
   good with nothing on the bus. The same order is now used for the output
   feeds and the audio tap. macOS is green end to end; Linux and Windows are
   one push from it as this is written. The release installers are unproven.
3. **A core exited silently once during mosaic teardown**, unattributed. One
   cause is now known: a source pipeline dropped without `stop` was disposed
   while PLAYING, which GStreamer answers with a critical and, with many
   pipelines in one process, a segmentation fault. Seen locally when a test
   failed an assertion and let its mixer go; `InputPipeline` now takes its
   pipeline to NULL on drop, and eight runs of the core suite in a row were
   clean after it. The second cause, seen on macOS runners as an audio
   monitoring branch disposed while PLAYING with no test failing first, is
   the bin's own state walk putting a freshly NULLed element back up before
   it was removed; sixty of sixty runs under load reproduced it and zero
   with the fix. Every branch teardown now locks the element's state before
   NULL, the way `Encoder::detach` always did.
4. **Alpha graphics key to black**, because the graph is I420 throughout. The
   four edits needed are listed in `docs/reference/graphics.md`.
5. **Smaller gaps**, each with its file and line in the git history: only
   sources take a `node:` placement; `source.set {place}` is remove then add
   rather than a pad swap; `gmx plugin new --kind graphic` has no template;
   the composer's picker does not offer graphics; `SidecarFilter` refuses the
   container transport; `ext.preview = "full"` composites at thumbnail detail.

## Running it

    cargo build --release -p godwinmix
    ./target/release/godwinmix --example-config > godwinmix.toml
    GODWINMIX_TOKEN=secret ./target/release/godwinmix --config godwinmix.toml

Then http://127.0.0.1:8080/ and the token. `dev/smoke.sh` is the end to end
check, `dev/soak.sh` the long one, `docs/README.md` the index.
