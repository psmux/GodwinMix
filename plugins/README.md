# First party plugins

Each directory here is one plugin: a Rust crate built on `godwinmix-sdk`, with
its `gmx-plugin.toml`, schemas, `SKILL.md` and tests. They are workspace
members so `cargo build --workspace` builds them, and each is installable with
`gmx plugin add ./plugins/<name>`. They use only the public sidecar contract;
nothing here is reachable by a third party plugin author's code that is not
reachable by yours.

| Plugin | Kind | What it does |
|---|---|---|
| `osc/` | `service` | OSC in on UDP, and tally and programme back out |
| `tally/` | `service` | TSL UMD v5 to hardware tally lamps, over UDP or TCP |
| `director/` | `service` | cuts the programme by itself, on rules or with a model |

## Building them

A plugin here is a workspace member, so cargo puts its binary in the
workspace's `target/`, which is *above* the plugin directory. A manifest path
may not climb out of its own directory, and the validator is right to refuse
one: an installed plugin is a directory somebody downloaded. So the binary is
staged into `plugins/<name>/bin/`, which is what `[run] bin` names and what
`gmx plugin add` would ship.

```sh
dev/plugins.sh build             # build and stage all three
dev/plugins.sh build --release
dev/plugins.sh test              # the above, then gmx plugin test on each
dev/plugins.sh clean
```

`plugins/*/bin/` is in `.gitignore`.

## Running a service plugin today

`SidecarService` and `SidecarDevice` are built in `godwinmix-host` and the
mixer does not yet construct either, so a `service` plugin is not started by
the core. Each of these three also runs by hand against a live core:

```sh
gmx-osc      --url http://127.0.0.1:8080 --token TOKEN --listen 0.0.0.0:9000
gmx-tally    --url http://127.0.0.1:8080 --token TOKEN --to 10.0.0.30:8900 --lamp cam1:0
gmx-director --url http://127.0.0.1:8080 --token TOKEN --min-hold 6
```

Each chooses its mode on `PluginEnv::started_by_core()`, so when the core does
start services, neither the binaries nor the manifests change.

## Testing them

```sh
cargo test -p gmx-osc -p gmx-tally -p gmx-director   # 116 unit tests, no core
dev/plugins.sh test                                  # the conformance harness
dev/integrations-live.sh                             # all three against a real core
```

`dev/integrations-live.sh` starts a core the way `dev/smoke.sh` does, adds two
test sources, and then sends an OSC take from a ten line Python sender, reads a
TSL packet with a Python listener and checks the lamp bits, and runs the
director for thirty seconds. Each plugin's README carries the recorded output.
## What is in here
| Directory | What it is |
|---|---|
| `camera/` | a USB or built in camera as a source |
| `audio-device/` | a microphone, a line input or a sound card as a source |
| `screen/` | a monitor, or part of one, as a source |
| `file-record/` | the programme recorded to a file |
| `capture-common/` | **a library, not a plugin.** What the four above share: choosing a capture element that this machine has, spelling the media transport the way the core reads it, watching the bus off the streaming thread, the device monitor, the programme FIFO and the free space reading. It has no `gmx-plugin.toml` and `gmx plugin add` has nothing to do with it |
[The first party plugin reference](../docs/reference/plugins.md) has the table
of what each one provides, where it runs and what is verified.
## Building one
These are workspace members, so `cargo build --release` puts their binaries in
the repository's own `target/`. `gmx plugin add` copies a plugin directory and
skips anything called `target`, so each plugin has a `./build` that stages its
binary at `bin/<name>`, which is where its manifest says it is. Run `./build`
before `gmx plugin add`.
