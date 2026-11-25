//! `godwinmix users`: the accounts, from a shell on the mixer's machine.
//!
//! Works on the file beside the config (`--config`, before `users`), with or
//! without the mixer running: a running mixer notices the change within a
//! second. Made for a server with no browser on it, and for an installer
//! that sets the first admin over SSH. A password is read from stdin or a
//! file, never from the command line, where `ps` and the shell history would
//! keep it.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use godwinmix_protocol::accounts::{Role, UserAddRequest, UserUpdateRequest};
use godwinmix_protocol::scope::ConfirmPolicy;

use crate::accounts::{store, Accounts};

#[derive(Args, Debug)]
pub struct UsersArgs {
    #[command(subcommand)]
    pub cmd: UsersCmd,
}

/// Where a new password comes from.
#[derive(Args, Debug, Clone)]
pub struct PasswordFrom {
    /// Read the password from standard input: the first line, without its
    /// line ending. `printf '%s' "$PW" | godwinmix users add sam --password-stdin`.
    #[arg(long, conflicts_with = "password_file")]
    pub password_stdin: bool,
    /// Read the password from this file: its first line.
    #[arg(long, value_name = "PATH")]
    pub password_file: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum UsersCmd {
    /// Every account: name, role, disabled, sessions.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Make an account. The first one must be an admin.
    Add {
        name: String,
        /// view, operate or admin.
        #[arg(long, default_value = "operate")]
        role: String,
        /// Ask for a confirmation before destructive calls.
        #[arg(long)]
        confirm: bool,
        #[command(flatten)]
        password: PasswordFrom,
    },
    /// Set a new password. Every session of the account ends.
    Passwd {
        name: String,
        #[command(flatten)]
        password: PasswordFrom,
    },
    /// Give an account another role: view, operate or admin.
    Role { name: String, role: String },
    /// Stop an account signing in, and end its sessions now.
    Disable { name: String },
    /// Let a disabled account sign in again.
    Enable { name: String },
    /// Remove an account and end its sessions now.
    Remove { name: String },
    /// The signed in sessions, or end some.
    Sessions {
        #[command(subcommand)]
        cmd: Option<SessionsCmd>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SessionsCmd {
    /// Every live session, or one account's.
    List {
        name: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// End sessions: one by --session, all of one account by name, or every
    /// session of everybody with --all.
    Revoke {
        name: Option<String>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

pub fn run(config: &Path, cmd: UsersCmd) -> Result<()> {
    if !config.exists() {
        bail!(
            "there is no config at {}. Pass the one the mixer runs with, before the subcommand: \
             godwinmix --config /etc/godwinmix/godwinmix.toml users list",
            config.display()
        );
    }
    let config = std::path::absolute(config).unwrap_or_else(|_| config.to_path_buf());
    let accounts = Accounts::open(Some(store::path_beside(&config)));
    let rpc = |e: godwinmix_protocol::error::RpcError| anyhow::anyhow!(e.message);
    match cmd {
        UsersCmd::List { json } => super::users_print::users(&accounts.list(), json),
        UsersCmd::Add { name, role, confirm, password } => {
            let role = role_of(&role)?;
            if accounts.list().is_empty() && role != Role::Admin {
                bail!("the first account has to be an admin, or nobody could manage the users from a page. Add it with --role admin.");
            }
            let taken = taken_in(&config);
            let confirm = if confirm { ConfirmPolicy::Required } else { ConfirmPolicy::None };
            let req = UserAddRequest { name, role, password: read_password(&password)?, confirm };
            let made = accounts.add(req, &|id| taken.iter().any(|t| t == id)).map_err(rpc)?;
            println!("Added {} ({}). Sign in at the mixer's address with that username and password.", made.user.name, made.user.role.as_str());
        }
        UsersCmd::Passwd { name, password } => {
            let done = accounts.reset_password(&name, &read_password(&password)?, None).map_err(rpc)?;
            println!("Changed the password of {}. Sessions ended: {}.", done.user.name, done.sessions_ended);
        }
        UsersCmd::Role { name, role } => {
            let done = accounts.update(update(&name, Some(role_of(&role)?), None)).map_err(rpc)?;
            println!("{} is now {}.", done.user.name, done.user.role.as_str());
        }
        UsersCmd::Disable { name } => {
            let done = accounts.update(update(&name, None, Some(true))).map_err(rpc)?;
            println!("Disabled {}. Sessions ended: {}.", done.user.name, done.sessions_ended);
        }
        UsersCmd::Enable { name } => {
            let done = accounts.update(update(&name, None, Some(false))).map_err(rpc)?;
            println!("Enabled {}.", done.user.name);
        }
        UsersCmd::Remove { name } => {
            let done = accounts.remove(&name).map_err(rpc)?;
            println!("Removed {}. Sessions ended: {}.", done.user.name, done.sessions_ended);
        }
        UsersCmd::Sessions { cmd } => match cmd.unwrap_or(SessionsCmd::List { name: None, json: false }) {
            SessionsCmd::List { name, json } => super::users_print::sessions(&accounts.sessions(name.as_deref(), None), json),
            SessionsCmd::Revoke { name, session, all } => {
                if name.is_none() && session.is_none() && !all {
                    bail!("say which sessions to end: a username, --session <id>, or --all for everybody's.");
                }
                let ended = accounts.sign_out(name.as_deref(), session.as_deref()).map_err(rpc)?;
                println!("Sessions ended: {ended}.");
            }
        },
    }
    Ok(())
}

fn update(name: &str, role: Option<Role>, disabled: Option<bool>) -> UserUpdateRequest {
    UserUpdateRequest { name: name.to_string(), role, disabled, confirm: None }
}

fn role_of(text: &str) -> Result<Role> {
    Role::parse(text).with_context(|| format!("'{text}' is not a role. Use view, operate or admin."))
}

/// The ids already in use by tokens in the config and the device file, so an
/// account never shares one with a token in the history.
fn taken_in(config: &Path) -> Vec<String> {
    let mut ids = vec!["open".to_string(), "default".into(), "station".into()];
    if let Ok(cfg) = godwinmix_core::config::Config::load(config) {
        ids.extend(cfg.tokens.iter().map(|t| t.id.clone()));
    }
    let devices = crate::devices::for_config(config);
    ids.extend(godwinmix_protocol::devices::DeviceRegistry::list(devices.as_ref()).into_iter().map(|d| d.id));
    ids
}

/// The first line of stdin or of a file, without its line ending.
fn read_password(from: &PasswordFrom) -> Result<String> {
    let text = match (&from.password_file, from.password_stdin) {
        (Some(path), _) => std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?,
        (None, true) => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).context("reading the password from stdin")?;
            text
        }
        (None, false) => bail!("give the password with --password-stdin or --password-file <path>. It is never taken on the command line, where the shell history would keep it."),
    };
    let line = text.split('\n').next().unwrap_or_default();
    Ok(line.strip_suffix('\r').unwrap_or(line).to_string())
}
