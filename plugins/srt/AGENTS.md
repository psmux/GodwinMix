# AGENTS.md

For a coding agent changing this plugin. Read this before editing anything.

## What this is

`srt/source`: a GodwinMix source plugin in Rust, built on `godwinmix-sdk` and
on `plugins/netkit` (the shared GStreamer plumbing for the four network
plugins). One process per source instance, started by the core. Control is
JSON-RPC 2.0 on stdin and stderr, one object a line, and the SDK runs that loop.

It is a byte mover. `srtsrc ! queue ! fdsink fd=1` and nothing else. The MPEG-TS
that arrives goes to the core exactly as it arrived, and the core demuxes and
decodes it on its own hardware aware path. Do not add a demuxer, a parser or a
decoder here: that would pay for the decode twice.

## Build and test

```sh
cargo test -p gmx-srt                      # unit tests plus a real sender on the loopback
cargo clippy -p gmx-srt --all-targets      # add no warning that was not there
dev/harness/stage-plugins.sh               # build and put the binary at plugins/srt/bin/
gmx plugin test plugins/srt --offline      # replay tests/transcript.jsonl, no core
dev/harness/publish.sh srt 9000 &          # something to receive
gmx plugin test plugins/srt                # the full conformance harness
```

The harness's checks 2 and 3 count frames, so they fail honestly with no sender
running. That is not a bug in the plugin.

## Where things are

| Path | What it is |
|---|---|
| `src/main.rs` | the `Source` implementation: the method bodies and the errors they return |
| `src/source.rs` | the pipeline, the bus watch, the health poll thread |
| `src/uri.rs` | settings to one `srt://` address, and the redaction that keeps the passphrase out of every log |
| `gmx-plugin.toml` | the manifest |
| `schemas/source.json` | the settings, JSON Schema draft 2020-12. Every surface renders this |
| `skills/source/SKILL.md` | what an agent operating the mixer needs to know |
| `tests/transcript.jsonl` | a recorded conversation with the core, replayed offline |
| `designer/icon.svg` | the icon in the add input gallery |

## The rules that matter

1. **stdout is media.** A `println!` in this process corrupts the stream. Log
   through the `Reporter` the SDK hands to `initialize`; it writes to stderr.
2. **The passphrase never reaches a string that gets printed.** It is set as a
   property on `srtsrc`, never put in the address, and `Settings::redacted` is
   the only form that leaves this process. There is a test for this. Do not
   build an address with the passphrase in it "just for the log line".
3. **Never block a streaming thread or the bus handler.** The bus is popped on
   its own thread in `netkit::pipe`, and all it does per message is set an
   atomic and write one line. The health poll is one property read a second.
4. **`configure` answers, never crashes.** A live SRT connection cannot move to
   a new address underneath the stream, so it returns `restart_required` with
   the reason. A stopped source takes the new settings outright.
5. **Every error names the next step.** Not "invalid mode" but the three modes
   and what each does. The harness does not check this; readers do.
6. **Add no dependency without a reason you can write down.** This crate is the
   SDK, netkit, gstreamer and serde_json. A URL crate is not worth it for the
   twelve lines in `uri.rs` that encode a query value.

## What is deliberately not here

* No sending. `srt/output` is built into the core at
  `crates/godwinmix-core/src/plugin/outputs/srt.rs` and stays there.
* No `keyframe-request` capability. SRT has no back channel a receiver can ask
  a keyframe on, so declaring it would be a lie. The core falls back to its own
  encoder GOP, which is the right answer.
* No `seek`. A live link has no timeline.
