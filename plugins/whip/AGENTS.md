# AGENTS.md

For a coding agent changing this plugin. Read this before editing anything.

## What this is

Two provides in one process image: `whip/output` sends the programme to a WHIP
endpoint, `whip/whep` receives a stream over WHEP. `GMX_PROVIDE` says which one
this process is, and `main` picks the handler from it. Built on
`godwinmix-sdk` and `plugins/netkit`.

The two share `src/settings.rs` because WHIP and WHEP are the same protocol
pointed in opposite directions and take the same four or five keys.

## Build and test

```sh
cargo test -p gmx-whip                     # unit tests; they skip where elements are missing
cargo clippy -p gmx-whip --all-targets     # add no warning that was not there
dev/harness/stage-plugins.sh               # build and stage plugins/whip/bin/
gmx plugin test plugins/whip --offline     # replay tests/transcript.jsonl
```

## Where things are

| Path | What it is |
|---|---|
| `src/main.rs` | both trait implementations, the method bodies and their errors |
| `src/output.rs` | the WHIP pipeline, the reconnect supervisor, the ICE state read |
| `src/whep.rs` | the WHEP pipeline and its health |
| `src/settings.rs` | the shared settings and the redaction |
| `schemas/output.json`, `schemas/whep.json` | the settings, JSON Schema draft 2020-12 |
| `skills/output/SKILL.md`, `skills/whep/SKILL.md` | what an agent operating the mixer needs |
| `tests/transcript.jsonl` | a recorded conversation, replayed offline |

## The rules that matter

1. **stdout is media** in `whip/whep`. A `println!` corrupts the stream. Log
   through the `Reporter`, which writes to stderr.
2. **An output READS its media.** `start.params.media` is a FIFO the core has
   already begun muxing the programme into. This is the one place in the plugin
   contract where media flows towards the plugin. Do not write to it.
3. **The token never reaches a printable string.** It goes to the signaller
   object and nowhere else; `redacted_endpoint()` is the only form that leaves
   this process, and `stats` answers `token: true`, never the token. There is a
   test. Keep it.
4. **Never re-encode the programme video.** `whipclientsink` takes H.264 on its
   request pad, so the core's encode is the only one. If the programme is some
   other codec the branch returns an error naming `codecs.toml`. Adding an
   encoder here would double the CPU of every show quietly.
5. **Never block a streaming thread or the bus handler.** The bus watch lives in
   `netkit::pipe` on its own thread. The reconnect supervisor sleeps in 250 ms
   slices so a shutdown during a fifteen second backoff is still prompt.
6. **The reconnect ceiling stays low.** An output that is down holds the core's
   outage buffer open. Raising `reconnect_max_ms` past a minute trades a short
   outage for a long one.
7. **`configure` answers, never crashes.** A WHIP session is one POST to one
   endpoint, so a new endpoint while running is `restart_required` with the
   reason. A stopped output takes it outright.

## What is deliberately not here

* No signalling server. WHIP exists so there does not have to be one.
* No `keyframe-request`. The core forwards `keyframe` to an output that declares
  it, and this plugin has nowhere to forward it to: the encode happens upstream
  in the core. Declaring it would be a lie.
* No retry loop in `whip/whep`. A WHEP session is negotiated during the state
  change, so an endpoint that is not there fails `start`, and the core's
  supervisor restarts the instance with its own backoff. That is what
  `restart-in-place` is for. The output needs its own supervisor because it
  holds a FIFO the core is writing into and cannot simply die.
