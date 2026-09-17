# Where GodwinMix stands

Written 2026-09-15 at the v0.2.0 tag. Read this first if you are picking the
work up fresh. The product requirements live outside this repository, in the
private `~/workspace/modulargodwinmix` (remote `psmux/modulargodwinmix`), and
must never be copied into this one.

## Built and merged

Everything in the plan's phases 0 to 6 except the items listed below. The
protocol and generated clients, the plugin host with fourteen first party
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
   worse (`crates/godwinmix-core/src/mixer/slots.rs:95`). Reproduce by running
   a core overnight with an output pointing at a closed port. This is the one
   defect that stops somebody using the product.

   One wedge is found and fixed, on 2026-09-16, by a Linux runner: the
   preview teardown took a tile's queue to NULL while its streaming thread
   was parked inside the compositor's sink pad and released that pad
   afterwards, so the join never returned. The pad is released first now,
   in the preview and in the two mosaic paths with the same order. Whether
   the overnight wedge with a reconnecting output is the same thing is not
   known; the soak still needs running. What is new either way: every call
   that waits on the mixer thread has a five second deadline and answers
   with the command holding the loop and for how long, `/api/status` and
   `/metrics` answer 503 with that instead of hanging, and a watchdog on the
   supervisor's timer logs it once. A wedge is now a diagnosable error
   rather than a silent hang.

   The second wedge, found by the soak on 2026-09-17 and reproducible in
   under a minute with `dev/soak.sh`, was the one that matters: removing a
   source took its branch's proxy source to NULL while a serialized
   allocation query was still travelling down that branch, and the query's
   tail was a slot thread parked in a compositor pad nobody had woken.
   Deactivating the pad needs the stream lock the query holds, so the mixer
   thread waited for a thread that was waiting for it, for as long as two
   minutes. A tee handing out or taking back a pad makes the source
   renegotiate, so every add and remove sent a fresh query down a live
   branch. A flush at the slot's queue on unbind and at the branch's video
   queue on detach frees the parked thread before any state change. After
   it, thirty six soak rounds in three minutes: no unanswered call, every
   removal inside 200 ms, threads and descriptors flat. Two bars still
   fail, both older: the programme stall gauge keeps one early 50 ms
   hiccup for the whole run, and resident memory grows about 3 MB a round
   with threads and descriptors flat, which reads as retained buffer pool
   heap rather than a leaked pipeline. Both records are in `bench/results`.
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
   NULL, the way `Encoder::detach` always did. The third cause, found by the
   ten minute soak on 2026-09-17 at round 61 with a crash report to prove it:
   the compositor's aggregate thread walks a pad's control bindings without
   the object lock (GStreamer's own source says so), and a settling
   transition removed a binding from the mixer thread under that walk. No
   binding is removed while the pipeline runs any more: one binding per pad
   and property is made the first time a transition drives it and kept for
   the pad's life, a transition rewrites the curve behind it, and settling
   collapses the curve and disables it. Two ten minute soaks after that ran
   to the end with the core up, descriptors and threads flat.

   The stall gauge's 50 ms was real, and is fixed: hiding a slot that still
   had a source bound to it shut its valve, and the programme compositor
   then waited out its whole upstream latency, one second, for a pad that
   had gone quiet, on every take that took a slot off air. A bound slot
   keeps feeding while hidden now and the gauge reads 34.3 ms, against a
   bar of 34; the last 0.4 ms is the source add and remove phase. Resident
   memory still grows, about 1 MB a second while the mosaic is up, and is
   measured to be GStreamer buffer pools on the programme path ratcheting
   their high water mark each time the mosaic taps and untaps the tee, not
   a leak: nothing is unreachable and no element, pad or pool outlives its
   round. Bounding the raw path's queues by buffers as well as by time
   would cap it and changes backpressure on the programme tee, so it waits
   for its own tests. Every record is in `bench/results`, and `dev/soak.sh
   --skip` bisects by phase.

   The scene preview on a Linux runner never produced a frame inside the
   API's two seconds, and once on a macOS runner. The mosaic's tile
   branches start at a proxysrc that answered no latency of its own, so
   the query crossed into the programme and came back with the programme
   compositor's one second budget: the mosaic held its first frame 1.26 s,
   the preview 450 ms, and a tile tee linked straight to a full mosaic pad
   starved the preview branch beside it. The runners' GStreamer 1.24 has
   no clock based start for a force live aggregator, so there the preview
   could not produce at all. The proxy now answers the latency itself, as
   every other boundary in the core does; the mosaic and preview declare
   375 ms and 250 ms and a test holds both under 500 ms.

   The first nightly soak on a hosted Linux runner, sixty minutes on
   2026-09-17: 720 rounds, the core up at the end, no panic, no call
   unanswered, descriptors and threads flat. Two bars missed: one 51 ms
   stall at round 495, forty minutes in, and resident memory up 52 percent
   over the hour, which is the buffer pool ratchet above. The overnight
   wedge this item opened with has not been seen since the fixes above.
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
