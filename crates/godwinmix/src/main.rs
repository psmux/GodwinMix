//! The `godwinmix` binary.
//!
//! Everything is in the library so that `gmx`, the short name, is the same
//! program rather than a copy of it. See `lib.rs`.

fn main() -> anyhow::Result<()> {
    godwinmix::main_with_room()
}
