//! `gmx`: the short name for `godwinmix`, the same program.
//!
//! It exists because an operator types this one all day and a symlink does not
//! survive a zip file, a Docker COPY or a Windows artifact.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    godwinmix::run().await
}
