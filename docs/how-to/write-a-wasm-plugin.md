# Write a WASM plugin

A plugin that decides things rather than carrying pictures can run as a
WebAssembly component inside the core: sandboxed, with a fuel allowance and a
deadline on every call, and no way to touch a frame. This is the shortest path
from nothing to a passing harness.

Eight commands. The first two are once per machine.

## Before you start

```
rustup target add wasm32-wasip2
```

That is the whole toolchain. `wasm32-wasip2` emits a component at link time, so
there is no `cargo component` and no `wasm-tools` to install.

Your core has to carry the WebAssembly host, which is off by default because it
adds 11.9 MB to the binary:

```
cargo build --release --features wasm
gmx doctor | grep 'wasm host'
```

You want the line that reads `ok wasm host wasmtime 36 (LTS), ...`. If it says
`warn`, the build has no host and nothing below will start.

## 1. Make one

```
gmx plugin new shot-clock --lang wasm --description "Refuse a take that comes too soon"
cd shot-clock
```

`--kind` defaults to `service` here. A component carries no media, so `source`,
`output` and `filter` are refused with a message naming the placement that does
(`sidecar`).

You get:

```
gmx-plugin.toml         placements = ["wasm"], [run] wasm = "plugin.wasm"
src/lib.rs              one trait, one macro
schemas/settings.json   the settings UI, everywhere
skills/service/SKILL.md what an agent reads before using it
tests/transcript.jsonl  a recorded conversation, replayed with no core
check                   build and replay, under thirty seconds
```

## 2. Build it

```
./check
```

That builds for `wasm32-wasip2`, copies the component to `plugin.wasm` beside
the manifest, and replays the transcript. `plugin.wasm` is the file `[run]
wasm` names, and it has to sit there because `gmx plugin add` copies the
directory.

## 3. Write the decision

`src/lib.rs` starts with a hook that allows everything. Here is a whole plugin
that refuses a take inside eight seconds:

```rust
use godwinmix_sdk_wasm::{allow, export_service, now_ms, refuse, Hello, Ready, Service};
use serde_json::{json, Value};

pub struct ShotClock {
    window_ms: u64,
    last_ms: Option<u64>,
}

impl Service for ShotClock {
    fn initialize(hello: &Hello) -> Result<(Self, Ready), String> {
        Ok((
            ShotClock { window_ms: hello.param_u64("min_hold_ms", 8_000), last_ms: None },
            Ready::new("shot-clock", "0.1.0").hook("take.before"),
        ))
    }

    fn hook(&mut self, name: &str, _payload: &Value) -> Result<Value, String> {
        if name != "take.before" {
            return Ok(json!({}));
        }
        let now = now_ms();
        let gone = now.saturating_sub(self.last_ms.unwrap_or(0));
        if self.last_ms.is_some() && gone < self.window_ms {
            return Ok(refuse(format!(
                "the last take was {gone} ms ago and this channel holds a shot for {} ms. \
                 Wait {} ms.",
                self.window_ms,
                self.window_ms - gone
            )));
        }
        self.last_ms = Some(now);
        Ok(allow())
    }
}

export_service!(ShotClock);
```

Two things to notice, because they are where plugins go wrong.

`Ready::new(..).hook("take.before")` is what makes the hook fire. The
`[hooks]` table in the manifest says the core should ask, and this says the
plugin will answer; the core calls nothing that is not in both.

The reason on a refusal is the whole of what an operator or an agent reads.
Name what is wrong and what to do, in that order. `-32003` is retryable and an
agent will act on the number in the message.

## 4. Test it without a core

```
./check
```

The transcript in `tests/transcript.jsonl` is a recorded conversation: `core`
lines are calls into the component, `plugin` lines are what it must answer,
matched as a subset. Add the case you just wrote:

```jsonl
{"core": {"jsonrpc": "2.0", "id": 4, "method": "hook", "params": {"hook": "take.before", "payload": {}}}}
{"plugin": {"jsonrpc": "2.0", "id": 4, "result": {"allow": true}}}
{"core": {"jsonrpc": "2.0", "id": 5, "method": "hook", "params": {"hook": "take.before", "payload": {}}}}
{"plugin": {"jsonrpc": "2.0", "id": 5, "result": {"allow": false, "reason": "*"}}}
```

`"*"` matches any value, which is what you want for a message you will reword.
No core, no sockets, no clock: this runs on any CI box in seconds.

## 5. Test it against the real host

```
gmx plugin test .
```

```
  ok   manifest               shot-clock v0.1.0: 1 provide(s), 0 tool(s), every path and schema in place
  ok   component              loaded and hand shook in 1802 ms; tools: none; hooks: take.before
  ok   media                  a service provide at the `wasm` placement carries no media, so checks 2, 3 and 6 do not apply
  ok   configure              2 settings examples were taken
  ok   service                health is ok; 1 hooks answered: take.before
```

The checks that are about a process are not run, because there is no process.
What is left runs in this process against the real wasmtime host.

## 6. Install it

```
gmx plugin add .
```

Then, in `godwinmix.toml`:

```toml
[plugins.shot-clock]
min_hold_ms = 8000
```

Restart the core, or `plugin.reload shot-clock` while it is running. A take
inside the window now answers:

```
-32003  a take.before hook refused this take: shot-clock: the last take was 1200 ms ago
        and this channel holds a shot for 8000 ms. Wait 6800 ms.
```

## 7. Prove it does something

The honest test of a policy plugin is a recorded show replayed with and without
it:

```
gmx session replay my-show.jsonl
gmx session replay my-show.jsonl --with-plugin ./shot-clock
```

The second is the first with your plugin installed in the test core. The
difference between the two reports is what your plugin does, measured rather
than described. `plugins/min-hold` in this repository is the worked example and
`tests/sessions/min-hold-wasm.jsonl` is its recording.

## A transition instead

Same shape, a different trait. `examples/wasm-ease` is the whole thing:

```rust
use godwinmix_sdk_wasm::{export_transition, Answer, Hello, Ready, Transition};
use serde_json::Value;

pub struct Ease;

impl Transition for Ease {
    fn initialize(_hello: &Hello) -> Result<(Self, Ready), String> {
        Ok((Ease, Ready::new("wasm-ease", "0.1.0")))
    }

    fn render(&mut self, _request: &Value) -> Result<Answer, String> {
        Ok(Answer::smoothstep(32))
    }
}

export_transition!(Ease);
```

Answer with a curve, not with per frame pad values, whenever you can. A curve
is asked for once per take and then bound to the compositor pads as a control
source, so your component is out of the loop for the whole transition. Set the
manifest's provide to `kind = "transition"` and take with it by the plugin's
name:

```
program.take {scene: "wide", transition: {type: "wasm-ease", duration_ms: 300}}
```

## What you cannot do

| | |
|---|---|
| touch a frame, a pad or a buffer | there is none here. Write a `sidecar` filter |
| open a file or a socket | unless the manifest declares `wasi = [...]` **and** the operator writes `[plugins] allow_wasi = ["shot-clock"]` |
| call a core method that changes something | except `program.take`, and only if you asked for a `take.*` hook |
| block, sleep or poll | a call has a fuel allowance and a deadline, and there is nothing to wait for |

The refusal for each of those names the next step. The full allow list and the
grant table are in [the WASM reference](../reference/wasm.md).

## When it will not build

**`can't find crate for core`** with `wasm32-wasip2` installed: another rust is
first on your PATH. The `check` script prefers rustup's; if you are building by
hand, `RUSTC=$(rustup which --toolchain stable rustc) cargo build ...`.

**`Library not loaded: @rpath/libLLVM.dylib`** on macOS: rustup's `rust-lld`
looks for it one directory away from where it is. `check` sets
`DYLD_FALLBACK_LIBRARY_PATH` for you; by hand it is
`export DYLD_FALLBACK_LIBRARY_PATH=$(dirname $(dirname $(rustup which rustc)))/lib`.

**`is not a WebAssembly component`**: you built for `wasm32-wasip1`. That
target emits a module, not a component. Use `wasm32-wasip2`.

## See also

* [Plugins as WebAssembly components](../reference/wasm.md): the world, the
  method table, the limits, the binary size.
* [Why WASM is not on the frame path](../explanation/why-wasm-is-not-on-the-frame-path.md).
* [Hooks](hooks.md) for what each hook is fired on and what it may answer.
* [Write a source plugin](write-a-source-plugin.md) when you do need media.
