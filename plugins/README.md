# First party plugins

Each directory here is one plugin: a Rust crate built on `godwinmix-sdk`, with
its `gmx-plugin.toml`, schemas, `SKILL.md` and tests. They are workspace
members so `cargo build --workspace` builds them, and each is installable with
`gmx plugin add ./plugins/<name>`. They use only the public sidecar contract;
nothing here is reachable by a third party plugin author's code that is not
reachable by yours.

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
