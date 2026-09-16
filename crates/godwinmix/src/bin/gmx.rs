//! `gmx`: the short name for `godwinmix`, the same program.
//!
//! It exists because an operator types this one all day and a symlink does not
//! survive a zip file, a Docker COPY or a Windows artifact.

fn main() -> anyhow::Result<()> {
    godwinmix::main_with_room()
}
