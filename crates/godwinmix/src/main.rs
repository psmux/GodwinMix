//! The `godwinmix` binary.
//!
//! Everything is in the library so that `gmx`, the short name, is the same
//! program rather than a copy of it. See `lib.rs`.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    godwinmix::run().await
}
