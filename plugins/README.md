# First party plugins

Each directory here is one plugin: a Rust crate built on `godwinmix-sdk`, with
its `gmx-plugin.toml`, schemas, `SKILL.md` and tests. They are workspace
members so `cargo build --workspace` builds them, and each is installable with
`gmx plugin add ./plugins/<name>`. They use only the public sidecar contract;
nothing here is reachable by a third party plugin author's code that is not
reachable by yours.
