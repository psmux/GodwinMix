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
2. **CI has never run.** Every GitHub Actions job on `psmux/GodwinMix` is
   refused with a billing message, so the Linux and Windows builds, the Docker
   quickstart timing and the release installers are unproven on real runners.
   The workflows themselves are written and parse.
3. **A core exited silently once during mosaic teardown**, unattributed.
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
