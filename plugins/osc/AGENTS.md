# AGENTS.md

For a coding agent changing this plugin. Read this before touching anything.

## What this is

A GodwinMix `service` plugin in Rust: no media, control plane only. One
process. The core starts it and talks JSON-RPC 2.0 on stdin and stderr, one
object per line, and the SDK runs that loop. Everything it actually does it
does over the core's `/rpc`, whose URL arrives in `GMX_RPC` and whose token
arrives in `GMX_TOKEN`.

It also runs by hand with `--url` and `--token`, because the mixer does not yet
instantiate `service` plugins. Both modes run the same loop; `main.rs` chooses
between them on `PluginEnv::started_by_core()`.

## Build and test

```sh
cargo test -p gmx-osc                      # 34 unit tests, no sockets, no core
dev/plugins.sh build                       # build and stage bin/gmx-osc
gmx plugin test --offline plugins/osc      # replay tests/transcript.jsonl
gmx plugin test plugins/osc                # manifest, spawn, configure
dev/integrations-live.sh --only osc        # against a real core
```

`dev/plugins.sh` exists because this crate is a workspace member: cargo puts
the binary in the workspace's `target/`, which is above the plugin directory,
and a manifest path may not climb out of its own directory. The script copies
it into `bin/`, which is what `[run] bin` names.

## Where things are

| Path | What it is |
|---|---|
| `src/osc.rs` | OSC 1.0 on the wire. No mixer in it, no sockets |
| `src/map.rs` | an address becomes one `Action`, or a refusal saying why |
| `src/settings.rs` | `schemas/bridge.json`, read into a struct |
| `src/service.rs` | the sockets, the event stream, the loop between them |
| `src/main.rs` | the two modes, the `Service` trait, the `send_osc` tool |
| `gmx-plugin.toml` | what it provides and how the core starts it |
| `schemas/bridge.json` | the settings. Every surface renders this |
| `skills/bridge/SKILL.md` | what an agent operating the mixer needs to know |
| `tests/transcript.jsonl` | a recorded conversation, replayed offline |

## The rules that matter

1. **The behaviour lives in `osc.rs` and `map.rs`, and neither opens a socket.**
   That is why the tests are fast and complete. A new address is a new arm in
   `action_for` and a new test beside it. Do not put a decision in
   `service.rs`.
2. **The listening socket and the sending socket are separate, on purpose.**
   Sending a datagram to a port nobody is listening on earns an ICMP
   unreachable that the kernel reports on the *next* operation on that socket.
   On one socket, a tally message to a tablet that is asleep kills the listener
   that takes sources. Do not merge them back.
3. **A falsey button argument is a release, not a command.** A momentary button
   sends 1 down and 0 up. Acting on both takes twice.
4. **Every refusal names the next step.** `Refusal::Unknown` carries a sentence
   listing the addresses that would have worked. Keep it that way.
5. **`health` must answer fast.** The SDK answers it from the reader thread
   while a slow call is still running, and that only works if the method does
   not block.
6. **`configure` gets the full validated object, not a diff.** It goes into the
   watch channel and the worker picks it up; nothing needs a restart.
7. **No `println!`.** Log through the `Reporter` in sidecar mode and through
   `Stderr` by hand. Both are behind the `Log` trait.

## Changing it

* **A new address**: an arm in `map::action_for`, a variant in `Action`, an arm
  in `service::apply`, a row in the README table and in `SKILL.md`, and a test.
* **A new setting**: add it to `schemas/bridge.json` with a `description`, a
  `default` and at least one example, then read it in `Settings::from_value`.
  `gmx plugin test` feeds every example through `configure`, so an example that
  cannot be applied fails the check. Do not build a settings UI; the schema is
  the UI.
* **A new outgoing message**: an arm in `service::send_out`. Keep it behind
  `send_tally` or `send_program` so an operator can turn it off.
* **A new tool**: a `[[tools]]` block with an input schema and a description
  carrying one example call (the validator requires the word "example"), then
  an arm in `tool_call`.

## What not to do

* Do not add a dependency. This plugin has four and each earns its place. The
  OSC codec is here rather than `rosc` because it is 300 lines against 2,000
  plus `nom`, and it compiles in a fraction of a second on a Pi.
* Do not edit `tests/transcript.jsonl` to make a failing check pass. If the
  behaviour changed on purpose, re-record it and say so in the commit message.
* Do not make the listener do work that can block. A wedged surface must not be
  able to stall the loop, and nothing in this process may ever touch the media
  path.
