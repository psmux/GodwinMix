# min-hold

Refuse a take that comes too soon after the last one, with the time left in
the reason.

A worked example of tier W: a GodwinMix plugin as a WebAssembly component. It
runs inside the core, sandboxed, with a fuel allowance and a deadline on every
call, and it touches no media.

## What it does

One hook. `take.before` answers `{allow: false, reason}` when a take arrives
inside the window, and the reason names how long is left and which setting
changes it. `take.after` is how it learns when the last take landed.

The core has `[safety] min_hold_ms` and it is on by default. This is the same
rule written outside the core, which is the point: a station whose rule is not
the core's rule edits this and ships it, and the mixer never changes.

## Build and install

```
rustup target add wasm32-wasip2
dev/build-wasm.sh
gmx plugin add ./plugins/min-hold
```

`plugin.wasm` is committed, so a checkout with no wasm toolchain still runs the
tests and the replay that use it. `dev/build-wasm.sh` rebuilds it from `src/`.

The core has to carry the WebAssembly host:

```
cargo build --release --features wasm
gmx doctor | grep 'wasm host'
```

## Settings

```toml
[plugins.min-hold]
min_hold_ms = 8000
```

## Test it

```
gmx plugin test plugins/min-hold             # the harness, in process
gmx plugin test --offline plugins/min-hold   # the transcript, with no core
```

## See it do something

```
gmx session replay tests/sessions/min-hold-wasm.jsonl
gmx session replay tests/sessions/min-hold-wasm.jsonl --with-plugin ./plugins/min-hold
```

The recording is three takes, two of them two seconds apart. Without the plugin
every take lands. With it, the one inside the window is refused with `-32003`
and the programme has one change fewer. That difference is what the plugin
does, measured rather than described, and it is Phase 6's acceptance for the
tier.

`evals/cases/min-hold-wasm.json` is the same thing graded by the operator eval.

## See also

* [Write a WASM plugin](../../docs/how-to/write-a-wasm-plugin.md)
* [Plugins as WebAssembly components](../../docs/reference/wasm.md)
* [Why WASM is not on the frame path](../../docs/explanation/why-wasm-is-not-on-the-frame-path.md)
