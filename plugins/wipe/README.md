# wipe

A transition. The incoming scene slides in over the outgoing one, from any
edge, and can push the outgoing scene out ahead of it.

```bash
gmx plugin add ./plugins/wipe
gmx take --scene "wide" --transition wipe --duration 400
```

It is here as the worked example of the fourth plugin kind, which is the one
that is hardest to picture: a transition carries no media, opens no socket and
never touches a pipeline. The core gives it the pads on the way out and the
pads on the way in, asks what each one should look like at a fraction of the
way through, and binds the answers as control sources the compositor reads on
the frame. A transition written in Python over a pipe therefore lands on the
same frame a built in fade does; see `docs/reference/transitions.md`.

## Settings

| Key | Default | What it does |
|---|---|---|
| `direction` | `left` | `left`, `right`, `up` or `down`: which way the incoming scene travels |
| `push` | `true` | push the outgoing scene out of the far edge, rather than sliding over it |

```bash
gmx plugin settings set wipe '{"direction": "up", "push": false}'
```

## Building it

It is a member of the repository's workspace, so cargo builds it into the
workspace's own target directory, while the manifest names the path an
installed copy has:

```bash
cargo build --release -p gmx-wipe
mkdir -p plugins/wipe/bin && cp target/release/gmx-wipe plugins/wipe/bin/
```

`gmx plugin add ./plugins/wipe` then installs it. A copy fetched from git runs
the `[build]` block instead and needs neither line.

## What the tests check

`cargo test -p gmx-wipe` checks the arithmetic: the incoming scene is one
canvas away at the start, half a canvas away in the middle, and exactly on zero
at the end, in every direction.

`cargo test -p godwinmix-core --test transition_plugin` runs the conformance
harness against the built binary: the manifest, the handshake, `configure` with
every example the settings schema gives, and `render` at 0, 0.5 and 1. That
last one is the transition contract and nothing about wipes, so a dissolve or a
clock wipe written by somebody else passes the same check.
