# Working on {{name}}

A GodwinMix plugin written as a WebAssembly component.

## The loop

```
./check                 build the component and replay the transcript
gmx plugin test .       the core's own harness, in process
```

`./check` is the fast one and needs no core. Run it after every edit.

## The rules

* One trait (`Service` or `Transition`), one macro (`export_service!` or
  `export_transition!`). Do not hand write bindings.
* Everything on the boundary is JSON as a string, byte for byte what a
  process plugin would write on its line. Do not invent a different shape.
* `initialize` names every hook and tool. The core calls nothing that is not
  in that list, so adding a hook to `gmx-plugin.toml` alone does nothing.
* A hook that refuses answers `{"allow": false, "reason": "..."}` and the
  reason names what is wrong and what to do. Every other hook answers `{}`.
* A call has a fuel allowance and a deadline. Do not loop waiting for
  anything; there is nothing to wait for and the call will be cut.
* No media. There is no frame here and there never will be.

## Before you commit

* `./check` passes.
* `schemas/settings.json` carries an `examples` entry for every property, or
  the harness cannot exercise `configure`.
* `skills/` says what the plugin refuses and why.
