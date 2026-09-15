# {{name}}

{{description}}

A GodwinMix plugin as a WebAssembly component (tier W). It runs inside the
core, sandboxed, and touches no media.

## Build and install

```
rustup target add wasm32-wasip2
./check
gmx plugin add .
```

`./check` builds the component and copies it to `plugin.wasm`, which is what
`[run] wasm` in `gmx-plugin.toml` names. There is no `cargo component` and no
`wasm-tools`: the `wasm32-wasip2` target emits a component on its own.

## Where to start

* `src/lib.rs` is the plugin. One trait, one macro.
* `schemas/settings.json` is the settings UI, everywhere.
* `tests/transcript.jsonl` is the recorded conversation `./check` replays.
* `skills/` is what an AI agent reads before using this plugin.

## What a component can and cannot do

| Can | Cannot |
|---|---|
| answer hooks, tools, `configure` and `health` | touch a frame, a pad or a buffer |
| read the core (`program.get`, `source.list`, ...) | call a method that changes anything except `program.take` |
| log, and raise events | open a file or a socket, unless the manifest asks and the operator allows |

If you need media, this is the wrong placement. Write a `sidecar` plugin
instead: `gmx plugin new <name> --lang rust`.
