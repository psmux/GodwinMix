# wasm-ease

A crossfade that starts and ends gently, answered once as a curve.

The smallest interesting tier W transition, and the one worth copying. It
answers `render` with a smoothstep curve rather than with per frame pad values,
so the core asks it exactly once per take and then binds the curve to the
compositor pads as a control source. After that one call the component is out
of the loop entirely: the aggregator samples the control source per output
frame with no plugin in it.

## Build and install

```
rustup target add wasm32-wasip2
dev/build-wasm.sh
gmx plugin add ./examples/wasm-ease
```

The core has to have been built with `--features wasm`.

## Use it

```
program.take {scene: "wide", transition: {type: "wasm-ease", duration_ms: 300}}
```

`type` is the plugin's name, not `<plugin>/<provide>`.

## Settings

```toml
[plugins.wasm-ease]
points = 32          # how many points describe the curve
```

The core interpolates linearly between the points, so more than about 32 buys
nothing a viewer can see.

## What it proves

`crates/godwinmix-wasm/tests/frame_interval.rs` starves this component of fuel
on purpose and measures the programme's frame interval across a take. The take
falls back to the built in cut and the interval never moves, which is the whole
argument for the tier. The control beside it,
`frame_interval_curve.rs`, runs the same take with the component answering and
checks both the curve reaching the pads and the same interval.

## See also

* [Write a WASM plugin](../../docs/how-to/write-a-wasm-plugin.md)
* [Transitions](../../docs/reference/transitions.md)
* [Why WASM is not on the frame path](../../docs/explanation/why-wasm-is-not-on-the-frame-path.md)
