//! `godwinmix node` and `gmx node`.
//!
//! Two different jobs under one word, and the difference is which machine you
//! are standing at.
//!
//! On the core: `gmx node token`, `list`, `get`, `remove`, `discover`. These
//! are a thin client over the `node.*` methods and have no special access; the
//! web UI does exactly the same calls.
//!
//! On the other machine: `godwinmix node --core <address> --token <token>`.
//! That is the daemon. It enrols the first time, keeps one socket to the core,
//! and hosts whatever the core asks it to.

use anyhow::{Context, Result};
use clap::Subcommand;
use godwinmix_core::node;
use serde_json::{json, Value};

/// What `gmx node <cmd>` can be asked, on the core.
#[derive(Debug, Subcommand)]
pub enum Node {
    /// Mint a one time enrolment token for a node, and print the command to
    /// run on the other machine.
    Token {
        /// What the node will call itself. It goes in `place = "node:<name>"`.
        #[arg(long)]
        name: String,
        /// Where the node is, for the record.
        #[arg(long)]
        address: Option<String>,
        /// How long the token is good for, in seconds. Default one hour.
        #[arg(long)]
        ttl: Option<u64>,
    },
    /// Every node this core knows about.
    List,
    /// One node in full: its clock offset, its plugins, what it is hosting.
    Get {
        name: String,
    },
    /// Forget a node. Destructive: its certificate stops working.
    Remove {
        name: String,
        /// Say what would happen without doing it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Look for nodes on the local network over mDNS.
    Discover {
        /// How long to listen, in milliseconds.
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
    },
}

pub async fn run(base: &str, token: Option<&str>, cmd: Node) -> Result<()> {
    match cmd {
        Node::Token { name, address, ttl } => {
            let answer = call(
                base,
                token,
                "node.enrol",
                json!({ "name": name, "address": address, "ttl_secs": ttl }),
            )
            .await?;
            let minted = answer.get("token").and_then(Value::as_str).unwrap_or_default();
            println!("token for {name}:\n\n  {minted}\n");
            println!("On the other machine, run:\n");
            println!(
                "  godwinmix node --core <this core>:8443 --name {name} --enrol-token {minted}\n"
            );
            println!(
                "The token is good for one enrolment and expires. `gmx node list` shows the \
                 node once it has dialled in."
            );
        }
        Node::List => {
            let answer = call(base, token, "node.list", json!({})).await?;
            let nodes = answer.get("nodes").and_then(Value::as_array).cloned().unwrap_or_default();
            if nodes.is_empty() {
                let listening =
                    answer.get("listening").and_then(Value::as_bool).unwrap_or(false);
                println!(
                    "{}",
                    if listening {
                        "no nodes yet. `gmx node token --name <name>` mints one an enrolment token."
                    } else {
                        "this core is not listening for nodes. Add a [nodes] table with `listen` \
                         to the config and restart."
                    }
                );
                return Ok(());
            }
            println!(
                "{:<16} {:<9} {:>9} {:>10}  HOSTING",
                "NAME", "STATE", "BEAT", "CLOCK"
            );
            for n in nodes {
                let name = n.get("name").and_then(Value::as_str).unwrap_or("-");
                let state = n.get("state").and_then(Value::as_str).unwrap_or("-");
                let beat = n.get("heartbeat_age_ms").and_then(Value::as_u64).unwrap_or(0);
                let offset = n.get("clock_offset_ms").and_then(Value::as_f64).unwrap_or(0.0);
                let hosting = n
                    .get("instances")
                    .and_then(Value::as_array)
                    .map(|i| i.len())
                    .unwrap_or(0);
                let beat = if beat > 60_000 { "-".to_string() } else { format!("{beat} ms") };
                println!(
                    "{name:<16} {state:<9} {beat:>9} {offset:>9.2}ms  {hosting} instance(s)"
                );
            }
        }
        Node::Get { name } => {
            let answer = call(base, token, "node.get", json!({ "id": name })).await?;
            println!("{}", serde_json::to_string_pretty(&answer)?);
        }
        Node::Remove { name, dry_run } => {
            let answer =
                call(base, token, "node.remove", json!({ "id": name, "dry_run": dry_run })).await?;
            println!("{}", serde_json::to_string_pretty(&answer)?);
        }
        Node::Discover { timeout_ms } => {
            let answer =
                call(base, token, "node.discover", json!({ "timeout_ms": timeout_ms })).await?;
            let found = answer.get("found").and_then(Value::as_array).cloned().unwrap_or_default();
            if found.is_empty() {
                println!(
                    "nothing answered. A network without multicast finds nothing this way; put \
                     the node in the [nodes] table instead."
                );
                return Ok(());
            }
            for f in found {
                println!(
                    "{:<16} {:<22} role={} api={}",
                    f.get("name").and_then(Value::as_str).unwrap_or("-"),
                    f.get("address").and_then(Value::as_str).unwrap_or("-"),
                    f.get("role").and_then(Value::as_str).unwrap_or("-"),
                    f.get("api").and_then(Value::as_u64).unwrap_or(0),
                );
            }
        }
    }
    Ok(())
}

/// The arguments `godwinmix node` itself takes, on the other machine.
#[derive(Debug, clap::Args)]
pub struct Daemon {
    /// The core's node bridge: `host:port`, or a `wss://` URL. Giving this is
    /// what makes `godwinmix node` the daemon rather than a client.
    #[arg(long, env = "GODWINMIX_CORE")]
    pub core: Option<String>,
    /// A one time enrolment token from `gmx node token`. Only needed once:
    /// after the first run the certificate on disk is what gets this machine
    /// in. Spelled `--enrol-token` because `--token` on the same command is
    /// the core's bearer token, and confusing the two is exactly the mistake
    /// worth making impossible.
    #[arg(long = "enrol-token", env = "GODWINMIX_ENROL_TOKEN", hide_env_values = true)]
    pub enrol_token: Option<String>,
    /// What this machine calls itself. Defaults to its hostname.
    #[arg(long)]
    pub name: Option<String>,
    /// `net` or `ptp`. PTP needs a wired LAN and the gst-ptp-helper.
    #[arg(long, default_value = "net")]
    pub clock: String,
    /// The address the core should send media to, if this machine is behind
    /// something that rewrites addresses.
    #[arg(long)]
    pub media_host: Option<String>,
    /// Enrol, write the certificate down, and stop. Useful when the operator
    /// enrolling and the operator running the node are different people.
    #[arg(long)]
    pub enrol_only: bool,
}

/// Run the node daemon. This does not return until the process is stopped.
pub async fn serve(args: Daemon) -> Result<()> {
    gstreamer::init().context("initialising GStreamer on the node")?;
    let name = match args.name {
        Some(name) => name,
        None => hostname().context(
            "this machine has no name to use, so pass --name. It must match the name the \
             enrolment token was minted for",
        )?,
    };
    let core = args.core.context(
        "`godwinmix node` with no subcommand runs this machine as a node, and needs --core to \
         say which core. On the core itself, `gmx node token --name <name>` mints the token and \
         prints the whole command",
    )?;
    let options = node::daemon::Options {
        core,
        name,
        token: args.enrol_token,
        home: godwinmix_host::marketplace::home_dir(),
        clock: node::clock::Kind::parse(&args.clock).with_context(|| {
            format!("`{}` is not a clock. Write `net` or `ptp`.", args.clock)
        })?,
        media_host: args.media_host,
    };
    // The plugins this node has come from its own directory, exactly as the
    // core's do from its own. A node runs nothing the operator did not install
    // on it.
    for installed in godwinmix_core::plugin::loader::load_all(&Default::default()) {
        match &installed.problem {
            Some(problem) => tracing::warn!(plugin = installed.name(), "{problem}"),
            None => tracing::info!(
                plugin = installed.name(),
                version = installed.version(),
                provides = installed.provides.len(),
                "plugin"
            ),
        }
    }
    if args.enrol_only {
        let issued = node::daemon::ensure_identity(&options).await?;
        println!("enrolled as {}", issued.identity);
        println!("certificate written to {}", options.identity_path().display());
        return Ok(());
    }
    // mDNS is a convenience and never a dependency. A node that cannot
    // advertise still works; it just has to be listed in the core's config.
    let _advert = node::discovery::Advertisement::start(
        &options.name,
        "node",
        0,
        node::wire::BRIDGE_API,
    )
    .map_err(|e| tracing::debug!(error = %format!("{e:#}"), "not advertising over mDNS"))
    .ok();
    node::daemon::run(options).await
}

/// One call, through the same client every other subcommand uses.
///
/// No path is written down here: the route comes from the method table, so
/// `gmx node` is one more caller of the public contract rather than a second
/// way in.
async fn call(base: &str, token: Option<&str>, method: &str, params: Value) -> Result<Value> {
    let api = crate::ctl::Api::new(base, token)?;
    let id = params.get("id").and_then(Value::as_str).map(str::to_string);
    match id {
        Some(id) => api.call(method, Some(&id), &params).await,
        None => api.call(method, None, &params).await,
    }
}

fn hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .map(|h| h.split('.').next().unwrap_or(&h).to_lowercase())
        .filter(|h| !h.is_empty())
}
