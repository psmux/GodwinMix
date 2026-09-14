# AGENTS.md

For a coding agent changing this plugin. Read this before touching anything.

## What this is

A GodwinMix `service` plugin in Rust: no media, control plane only. It
subscribes to `event/tally` on the core's `/rpc` and turns every change into a
TSL UMD v5.0 packet on UDP or TCP.

It runs as a sidecar the core starts and also by hand with `--url` and
`--token`, because the mixer does not yet instantiate `service` plugins.
`main.rs` chooses on `PluginEnv::started_by_core()`.

## Build and test

```sh
cargo test -p gmx-tally                      # 32 unit tests
dev/plugins.sh build                         # build and stage bin/gmx-tally
gmx plugin test --offline plugins/tally
gmx plugin test plugins/tally
dev/integrations-live.sh --only tally        # against a real core
```

## Where things are

| Path | What it is |
|---|---|
| `src/tsl.rs` | TSL UMD v5.0 on the wire, encoder and decoder |
| `src/settings.rs` | `schemas/tally.json`, read into a struct |
| `src/sender.rs` | UDP and TCP, with the reconnect TCP needs |
| `src/service.rs` | the board of what each lamp shows, and the loop |
| `src/main.rs` | the two modes, the `Service` trait, the `test_lamp` tool |

## The rules that matter

1. **The protocol is little endian.** Everything in TSL is, which surprises
   people who have just come from OSC. `PBC` counts every byte after itself.
2. **The control word's four two bit fields are where the specification puts
   them**: right tally at bit 0, text at 2, left at 4, brightness at 6, and bit
   15 clear for a display message. There is a test asserting each one against a
   literal. Do not change the packing without changing that test on purpose.
3. **An ASCII label replaces what a lamp cannot show rather than dropping it.**
   Shortening the label shifts every later field and desynchronises a TCP
   receiver. There is a test for it.
4. **A decoder that guesses is worse than one that says nothing.** `decode`
   answers `None` on anything that is not a well formed v5.0 display message.
5. **Only mapped sources light.** A source with no entry in `lamps` produces no
   packet. A mixer with twelve sources and four lamps must not scatter packets
   at indices nobody assigned.
6. **The refresh timer is not optional dressing.** A lamp that was power cycled
   catches up on it without anybody taking a source, and it is also what
   recovers from a dropped datagram.
7. **The TCP sender connects on the first packet, not at startup.** A tally
   interface powered on after the mixer must not stop the mixer from starting.

## Changing it

* **A new setting**: `schemas/tally.json` with a `description`, a `default` and
  at least one example, then `Settings::from_value`. `gmx plugin test` feeds
  every example through `configure`.
* **A different lamp layout**: `service::packet_for` is the one function that
  decides what a lamp shows. It takes a `Board` and answers bytes, so a change
  there is a unit test, not a rig.
* **Another protocol** (TSL 3.1, for an older rig): a second module beside
  `tsl.rs` and a `protocol` value in the schema. Do not bend v5 into it.

## What not to do

* Do not add a dependency for the protocol. It is 200 lines with its tests.
* Do not send every lamp on every event. `Board::apply` answers with what
  moved, and that is what keeps a rig with forty lamps quiet.
* Do not edit `tests/transcript.jsonl` to make a failing check pass.
