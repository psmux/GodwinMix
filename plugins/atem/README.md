# atem

Blackmagic ATEM switchers for GodwinMix: several at once, each under its own
id; every bus, transition, keyer, aux output and macro as a tool; and modes
where a take in GodwinMix also cuts the switcher (`follow`) or a cut on the
switcher's panel moves GodwinMix (`lead`).

```sh
./build                      # stages bin/gmx-atem
gmx plugin add ./plugins/atem
```

```
connect {id: "atem-main", address: "192.0.2.10", name: "Main switcher"}
map {source: "cam-pulpit", input: 1}
mode {mode: "follow"}
preview {input: 2}
auto {}
```

By hand, with no core:

```sh
cargo run -p gmx-atem -- --switcher atem-main=192.0.2.10
list {}
cut {"switcher": "atem-main"}
```

No GStreamer and no media: UDP to the switchers and the public protocol to
the core. The protocol client is `crates/godwinmix-atem`.

* [How to connect an ATEM](../../docs/how-to/connect-an-atem.md)
* [Reference: tools, state, modes and tally](../../docs/reference/atem.md)
* [The ATEM network protocol](../../docs/reference/atem-protocol.md)

## Tests

```sh
cargo test -p gmx-atem -p godwinmix-atem
cargo clippy --all-targets -p gmx-atem -p godwinmix-atem
```

The tests run every tool against a fake switcher on loopback (the same one
`godwinmix-atem`'s tests use), check the bytes each tool puts on the wire,
run two switchers at once, and replay `tests/transcript.jsonl` against the
built binary. No switcher is needed.
