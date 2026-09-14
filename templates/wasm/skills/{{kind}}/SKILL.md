---
name: {{name}}
description: {{description}} Replace this sentence with the one an agent reads before deciding to load the rest of this file: say what the plugin does and name the words an operator would use when they want it.
---

# {{name}}/{{kind}}

{{description}}

It runs as a WebAssembly component inside the core: sandboxed, no media, a fuel
allowance and a deadline per call.

## Installing

```
gmx plugin add ./{{name}}
```

The core has to have been built with `--features wasm`. `gmx doctor` says
whether it was, on the `wasm host` line.

## Settings

| Key | Meaning |
|---|---|
| `example_ms` | replace this row with your own |

```toml
[plugins.{{name}}]
example_ms = 1000
```

## What it refuses, and why

Write down every case where this plugin answers `{allow: false}`, with the
reason text, so an operator reading the error and an agent handling it both
know what to do next.
